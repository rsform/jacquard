//! Fetching and decoding zstd dictionaries for the Jetstream v2 live stream.
//!
//! A dictionary is fetched from `getZstdDictionary`, identified from its
//! zstd header, and used to decode binary WebSocket frames containing JSON
//! text. Dictionaries are cached per instance and generation so
//! reconnections reuse them instead of re-downloading.

use core::fmt;
use std::collections::VecDeque;
use std::io;
use std::sync::Mutex;

use jacquard_common::xrpc::XrpcClient as _;
use jacquard_common::{deps::bytes, http_client::HttpClient};

use super::archive::{JetstreamClient, JetstreamError};

/// How many dictionary generations to keep cached. Dictionaries are
/// immutable per ID and rotate rarely; a small bound covers a rotation
/// window without unbounded growth.
const CACHE_CAPACITY: usize = 4;

/// Process-wide cache of fetched dictionaries, keyed by instance and
/// dictionary ID. Dictionaries are immutable per ID, so entries never
/// invalidate; the bound only stops unbounded growth across rotations.
static CACHE: Mutex<VecDeque<CacheEntry>> = Mutex::new(VecDeque::new());

struct CacheEntry {
    base: String,
    id: u32,
    bytes: bytes::Bytes,
}

/// The last-known current dictionary per instance, so `fetch(None)` on a
/// reconnect hits the cache instead of re-downloading. Rotations are
/// handled by the `UnknownZstdDictionary` recovery refetching by ID.
static CURRENT: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

fn cache_get(base: &str, id: u32) -> Option<ZstdDictionary> {
    let cache = CACHE.lock().expect("dictionary cache lock");
    cache
        .iter()
        .find(|e| e.base == base && e.id == id)
        .map(|e| ZstdDictionary {
            id,
            bytes: e.bytes.clone(),
        })
}

fn cache_put(base: &str, id: u32, bytes: bytes::Bytes) {
    let mut cache = CACHE.lock().expect("dictionary cache lock");
    if !cache.iter().any(|e| e.base == base && e.id == id) {
        cache.push_back(CacheEntry {
            base: base.to_string(),
            id,
            bytes,
        });
        while cache.len() > CACHE_CAPACITY {
            cache.pop_front();
        }
    }
}

fn current_get(base: &str) -> Option<u32> {
    CURRENT
        .lock()
        .expect("dictionary current lock")
        .iter()
        .find(|(b, _)| b == base)
        .map(|(_, id)| *id)
}

fn current_put(base: &str, id: u32) {
    let mut current = CURRENT.lock().expect("dictionary current lock");
    match current.iter_mut().find(|(b, _)| b == base) {
        Some(entry) => entry.1 = id,
        None => current.push((base.to_string(), id)),
    }
}

/// A fetched zstd dictionary: raw bytes plus the zstd-embedded ID used to
/// negotiate compressed frames.
#[derive(Debug, Clone)]
pub struct ZstdDictionary {
    /// The dictionary ID embedded in the dictionary header
    /// (`ZSTD_getDictID_fromDict`). A value of 0 cannot be negotiated by ID.
    pub id: u32,
    /// The raw dictionary bytes (RFC 8878 §5 structured dictionary).
    pub bytes: bytes::Bytes,
}

/// Errors from fetching or inspecting a dictionary.
#[derive(Debug)]
pub enum DictionaryError<E> {
    /// Transport / archive endpoint failure.
    Archive(JetstreamError<E>),
    /// The fetched bytes are not a loadable zstd dictionary.
    InvalidDictionary,
    /// The dictionary carries no zstd dictionary ID, so it cannot be
    /// negotiated via `zstdDictionary`.
    NoDictionaryId,
    /// A request for a specific generation returned a different dictionary.
    UnexpectedDictionaryId {
        /// Dictionary generation requested from the server.
        requested: u32,
        /// Dictionary generation embedded in the response bytes.
        received: u32,
    },
}

impl<E: fmt::Display> fmt::Display for DictionaryError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "{e}"),
            Self::InvalidDictionary => write!(f, "fetched bytes are not a zstd dictionary"),
            Self::NoDictionaryId => {
                write!(
                    f,
                    "dictionary carries no ID; cannot negotiate compressed frames"
                )
            }
            Self::UnexpectedDictionaryId {
                requested,
                received,
            } => write!(
                f,
                "requested dictionary {requested}, but server returned {received}"
            ),
        }
    }
}

impl<E: fmt::Display + fmt::Debug> std::error::Error for DictionaryError<E> {}

impl<E> From<JetstreamError<E>> for DictionaryError<E> {
    fn from(e: JetstreamError<E>) -> Self {
        Self::Archive(e)
    }
}

impl ZstdDictionary {
    /// Fetch a dictionary via `getZstdDictionary`, consulting the
    /// process-wide cache first.
    ///
    /// Pass `None` for the server's current dictionary or `Some(id)` for a
    /// specific generation. The cache is keyed per instance, so a
    /// reconnecting stream reuses its dictionary without another download;
    /// a rotation is picked up through `UnknownZstdDictionary` recovery.
    pub async fn fetch<C: HttpClient + Sync>(
        client: &JetstreamClient<C>,
        id: Option<i64>,
    ) -> Result<Self, DictionaryError<C::Error>> {
        let base = client.base_uri().await.as_str().to_string();
        let requested_id = id
            .map(|id| u32::try_from(id).map_err(|_| DictionaryError::NoDictionaryId))
            .transpose()?;
        let cached_id = requested_id.or_else(|| current_get(&base));
        if let Some(id) = cached_id {
            if let Some(cached) = cache_get(&base, id) {
                return Ok(cached);
            }
        }
        let bytes = client
            .get_zstd_dictionary(requested_id.map(i64::from))
            .await?;
        let dict = Self::from_bytes(bytes)?;
        if let Some(requested) = requested_id
            && requested != dict.id
        {
            return Err(DictionaryError::UnexpectedDictionaryId {
                requested,
                received: dict.id,
            });
        }
        cache_put(&base, dict.id, dict.bytes.clone());
        current_put(&base, dict.id);
        Ok(dict)
    }

    /// Refetch a specific generation without consulting the process cache.
    /// Rotation recovery uses this after the server rejects cached bytes.
    pub async fn refetch<C: HttpClient + Sync>(
        client: &JetstreamClient<C>,
        id: i64,
    ) -> Result<Self, DictionaryError<C::Error>> {
        let requested = u32::try_from(id).map_err(|_| DictionaryError::NoDictionaryId)?;
        let base = client.base_uri().await.as_str().to_string();
        let bytes = client.get_zstd_dictionary(Some(id)).await?;
        let dict = Self::from_bytes(bytes)?;
        if dict.id != requested {
            return Err(DictionaryError::UnexpectedDictionaryId {
                requested,
                received: dict.id,
            });
        }
        cache_put(&base, dict.id, dict.bytes.clone());
        current_put(&base, dict.id);
        Ok(dict)
    }

    /// Inspect raw dictionary bytes, extracting the embedded dictionary
    /// ID.
    pub fn from_bytes<E>(bytes: bytes::Bytes) -> Result<Self, DictionaryError<E>> {
        // A loadable dictionary must at minimum carry the zstd dict
        // magic; get_dict_id_from_dict rejects anything else.
        let id = zstd::zstd_safe::get_dict_id_from_dict(&bytes)
            .ok_or(DictionaryError::InvalidDictionary)?
            .get();
        if id == 0 {
            return Err(DictionaryError::NoDictionaryId);
        }
        Ok(Self { id, bytes })
    }

    /// Whether these bytes are the vendored v1 dictionary generation
    /// (id 1612007021). Divergence means the v2 server has rotated; the
    /// fetched generation is cached and used regardless.
    pub fn is_vendored_v1(&self) -> bool {
        self.bytes == jacquard_common::xrpc::subscription::VENDORED_ZSTD_DICTIONARY
    }

    /// Decompress one zstd frame through this dictionary into immutable bytes.
    /// Reading stops one byte beyond `max_size` so oversized output is rejected
    /// without allocating the full advertised frame.
    pub fn decompress_frame(&self, frame: &[u8], max_size: usize) -> io::Result<bytes::Bytes> {
        let mut decoder = zstd::Decoder::with_dictionary(frame, &self.bytes)?;
        let mut out = Vec::new();
        let read_limit = u64::try_from(max_size)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut limited = io::Read::take(&mut decoder, read_limit);
        io::Read::read_to_end(&mut limited, &mut out)?;
        if out.len() > max_size {
            return Err(io::Error::other("frame exceeds decompression cap"));
        }
        Ok(out.into())
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use jacquard_common::deps::{bytes, fluent_uri};

    use super::*;

    /// The real server's current dictionary, as captured from a live
    /// instance.
    fn server_dictionary() -> bytes::Bytes {
        bytes::Bytes::from(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/jetstream/testdata/dict.bin"
            ))
            .expect("fixture present"),
        )
    }

    /// Round-trip against the real server dictionary: extract its ID,
    /// compress a frame with it, decompress through
    /// [`ZstdDictionary`]. No network.
    #[test]
    fn dictionary_frame_matches_uncompressed() {
        let dict = ZstdDictionary::from_bytes::<Infallible>(server_dictionary())
            .expect("server dictionary loads");
        assert_eq!(dict.id, 20260811);

        let payload = br#"{"$type":"message","payload":{"did":"did:plc:aaa"}}"#;

        let with_dict = {
            let mut encoder =
                zstd::Encoder::with_dictionary(Vec::new(), 0, &dict.bytes).expect("encoder");
            use std::io::Write as _;
            encoder.write_all(payload).expect("write");
            encoder.finish().expect("frame")
        };

        let decompressed = dict
            .decompress_frame(&with_dict, payload.len())
            .expect("decompress");
        assert_eq!(decompressed, payload.as_slice());
        assert!(
            dict.decompress_frame(&with_dict, payload.len() - 1)
                .is_err()
        );
    }

    #[tokio::test]
    async fn fetch_specific_generation_sends_id_and_rejects_mismatch() {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/xrpc/network.bsky.jetstream.getZstdDictionary"))
            .and(query_param("id", "7"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(server_dictionary()))
            .expect(1)
            .mount(&server)
            .await;
        let base = fluent_uri::Uri::parse(server.uri()).expect("uri");
        let client = JetstreamClient::new(reqwest::Client::new(), base, None);

        let error = ZstdDictionary::fetch(&client, Some(7))
            .await
            .expect_err("returned generation differs");
        assert!(matches!(
            error,
            DictionaryError::UnexpectedDictionaryId {
                requested: 7,
                received: 20260811
            }
        ));
    }

    #[test]
    fn cache_roundtrip_per_instance() {
        let dict = ZstdDictionary::from_bytes::<Infallible>(server_dictionary())
            .expect("server dictionary loads");
        cache_put("https://a.test", dict.id, dict.bytes.clone());
        current_put("https://a.test", dict.id);

        assert!(cache_get("https://a.test", dict.id).is_some());
        assert!(
            cache_get("https://b.test", dict.id).is_none(),
            "dictionary cache must be keyed per instance"
        );
        assert_eq!(current_get("https://a.test"), Some(dict.id));
        assert_eq!(current_get("https://b.test"), None);
    }
}
