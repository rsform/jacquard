//! Live `subscribeEvents` stream: the low-level, non-managed primitive.
//!
//! Each stream uses one connection with no reconnect or backoff; callers
//! own connection management and error handling.
//!
//! The stream negotiates the lexicon-declared `xrpc.v1.json` subprotocol
//! at the upgrade (via the subscription layer's `SUBPROTOCOL` wiring) and
//! decodes proposal-0015 JSON envelopes into the generated
//! `SubscribeEventsMessage` union. A rejected upgrade surfaces as a
//! [`HandshakeError`] carrying status and body, so `CursorTooOld` /
//! `UnknownZstdDictionary` recovery values are inspectable by the caller.

use core::fmt;
use std::str::FromStr;

use jacquard_api::network_bsky::jetstream::subscribe_events::{
    SubscribeEvents, SubscribeEventsError, SubscribeEventsMessage,
};
use jacquard_common::BosStr;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::stream::{StreamError, StreamErrorKind};
use jacquard_common::websocket::WebSocketClient;
use jacquard_common::websocket::WebSocketError;
use jacquard_common::xrpc::SubscriptionExt;
#[cfg(not(target_arch = "wasm32"))]
use n0_future::stream::Boxed;
#[cfg(target_arch = "wasm32")]
use n0_future::stream::BoxedLocal as Boxed;
use serde::de::DeserializeOwned;

use super::archive::JetstreamClient;
use super::plan::{CollectionFilter, ReplayFilters};
use jacquard_common::http_client::HttpClient;
use jacquard_common::xrpc::XrpcClient as _;

/// A rejected WebSocket upgrade, with the parts recovery needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeError {
    /// The HTTP status of the rejection.
    pub status: u16,
    /// The typed error parsed from the body via the generated
    /// `SubscribeEventsError` (its variants carry the message strings).
    pub error: SubscribeEventsError,
    /// The recovery value the documented error formats carry: the
    /// retention floor seq for `CursorTooOld`, or the current dictionary
    /// ID for `UnknownZstdDictionary`.
    pub recovery_value: Option<i64>,
    /// The bounded rejection body preserved by the WebSocket transport.
    pub body: jacquard_common::deps::bytes::Bytes,
}

impl core::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "websocket upgrade rejected with HTTP {}: {}",
            self.status, self.error
        )
    }
}

impl std::error::Error for HandshakeError {}

impl HandshakeError {
    /// Extract a rejection from an error containing the shared
    /// [`WebSocketError`]. The body parses through the generated error enum;
    /// the recovery value is extracted from the some message markers
    /// (pinned upstream formats: `"... below lookback floor %d; ..."` for
    /// `CursorTooOld`, `"... current dictionary id is %d (fetch it via
    /// getZstdDictionary and reconnect)"` for `UnknownZstdDictionary`) because
    /// Go hates structured errors.
    fn from_error(e: &(dyn core::error::Error + 'static)) -> Option<Self> {
        let ws = e
            .downcast_ref::<WebSocketError>()
            .or_else(|| e.source().and_then(|s| s.downcast_ref::<WebSocketError>()))?;
        let WebSocketError::HandshakeRejected { status, body, .. } = ws else {
            return None;
        };
        let error: SubscribeEventsError = serde_json::from_slice(body).ok()?;

        let message = match &error {
            SubscribeEventsError::CursorTooOld(msg)
            | SubscribeEventsError::UnknownZstdDictionary(msg) => msg.as_deref(),
            _ => None,
        };
        // The integer following the marker is the recovery value; the
        // integer preceding it (the rejected cursor / dictionary id) is
        // not.
        let marker = match &error {
            SubscribeEventsError::CursorTooOld(_) => "lookback floor ",
            SubscribeEventsError::UnknownZstdDictionary(_) => "current dictionary id is ",
            _ => "",
        };
        let recovery_from_marker = |text: &str| -> Option<i64> {
            let tail = text.split(marker).nth(1)?;
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse().ok()
            }
        };

        let recovery_value = message.and_then(recovery_from_marker);

        Some(Self {
            status: status.as_u16(),
            error,
            recovery_value,
            body: body.clone(),
        })
    }
}

/// Errors from establishing or reading a live stream.
#[derive(Debug)]
pub enum LiveError<E> {
    /// The WebSocket upgrade was rejected; inspect the
    /// [`HandshakeError`] fields for `CursorTooOld` (retention floor) or
    /// `UnknownZstdDictionary` (current dictionary ID) recovery.
    Handshake(HandshakeError),
    /// A recovery-bearing handshake error omitted or malformed its recovery value.
    MalformedRecoveryPayload(HandshakeError),
    /// The archive client's base URI cannot be converted to a WebSocket URI.
    InvalidBaseScheme(String),
    /// A non-rejection transport error from the underlying client.
    Transport(E),
    /// A non-connection frame failed to decode into the generated message union.
    Decode(StreamError),
    /// The stream failed mid-connection (transport, protocol, or framing error).
    Stream(StreamError),
    /// The stream closed (clean close frame or transport EOF).
    Closed,
}

impl<E: core::fmt::Display> core::fmt::Display for LiveError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Handshake(e) => write!(f, "{e}"),
            Self::MalformedRecoveryPayload(e) => {
                write!(f, "malformed recovery payload in {e}")
            }
            Self::InvalidBaseScheme(scheme) => {
                write!(f, "unsupported Jetstream base URI scheme {scheme:?}")
            }
            Self::Transport(e) => write!(f, "live stream transport error: {e}"),
            Self::Decode(e) => write!(f, "frame decode error: {e}"),
            Self::Stream(e) => write!(f, "stream failed: {e}"),
            Self::Closed => write!(f, "stream closed"),
        }
    }
}

impl<E: core::fmt::Display + core::fmt::Debug> std::error::Error for LiveError<E> {}

/// Options for establishing a live stream.
#[derive(Debug, Clone, Default)]
pub struct LiveOptions {
    /// Resume position (inclusive). The server replays `seq >= cursor`.
    pub cursor: Option<i64>,
    /// Skip events whose uncompressed frame exceeds this many bytes. A value
    /// of 0 disables the limit.
    pub max_message_size_bytes: Option<i64>,
}

/// An established low-level `subscribeEvents` stream.
///
/// Single connection: when it ends, [`LiveStream::next`] returns
/// [`LiveError::Closed`] and the caller decides what to do. This type has
/// no reconnect/backoff logic by design.
pub struct LiveStream<E, S: BosStr> {
    rx: Boxed<Result<SubscribeEventsMessage<S>, StreamError>>,
    _transport: core::marker::PhantomData<fn() -> E>,
}

fn websocket_base(base: &Uri<String>) -> Result<Uri<String>, String> {
    let scheme = match base.scheme().as_str() {
        "http" => "ws",
        "https" => "wss",
        "ws" => "ws",
        "wss" => "wss",
        scheme => return Err(scheme.to_owned()),
    };
    let suffix = base
        .as_str()
        .split_once("://")
        .map(|(_, suffix)| suffix)
        .ok_or_else(|| base.scheme().as_str().to_owned())?;
    Uri::parse(format!("{scheme}://{suffix}")).map_err(|_| base.scheme().as_str().to_owned())
}

/// Establish a live `subscribeEvents` connection with dictionary
/// compression negotiated by default.
///
/// Fetches the server's current zstd dictionary, connects with
/// `zstdDictionary=<id>`, and decodes compressed binary frames through
/// it. On an `UnknownZstdDictionary` rejection (the server rotated
/// between fetch and upgrade) the named generation is refetched once and
/// the connection retried. A failed refetch, or a server that reports the
/// same dictionary ID it just rejected, falls back to
/// [`subscribe_events_uncompressed`]. Without the `zstd` feature this is an
/// uncompressed connect.
pub async fn subscribe_events<C, W, S>(
    client: &JetstreamClient<C>,
    ws_client: &W,
    filters: &ReplayFilters,
    options: LiveOptions,
) -> Result<LiveStream<W::Error, S>, LiveError<W::Error>>
where
    C: HttpClient + Sync,
    S: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    W: WebSocketClient,
    <S as FromStr>::Err: std::fmt::Debug,
{
    let archive_base = client.base_uri().await;
    let base = websocket_base(&archive_base).map_err(LiveError::InvalidBaseScheme)?;
    #[cfg(feature = "zstd")]
    {
        use super::dictionary::ZstdDictionary;
        use jacquard_api::network_bsky::jetstream::subscribe_events::SubscribeEventsError;
        match ZstdDictionary::fetch(client, None).await {
            Ok(dictionary) => {
                match subscribe_events_with_dictionary(
                    ws_client,
                    &base,
                    filters,
                    options.clone(),
                    &dictionary,
                )
                .await
                {
                    Ok(stream) => return Ok(stream),
                    Err(LiveError::Handshake(h)) => {
                        if matches!(h.error, SubscribeEventsError::UnknownZstdDictionary(_)) {
                            if let Some(current_id) = h.recovery_value
                                && u32::try_from(current_id).ok() != Some(dictionary.id)
                                && let Ok(current) =
                                    ZstdDictionary::refetch(client, current_id).await
                            {
                                return subscribe_events_with_dictionary(
                                    ws_client, &base, filters, options, &current,
                                )
                                .await;
                            }
                        } else {
                            return Err(LiveError::Handshake(h));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(_) => {}
        }
    }
    subscribe_events_uncompressed(ws_client, &base, filters, options).await
}

/// Establish a live `subscribeEvents` connection over any WebSocket
/// client, without compression: frames arrive as JSON text and decode
/// directly. Frames carry no dictionary negotiation; use
/// [`subscribe_events`] (the default) for compressed frames or
/// [`subscribe_events_with_dictionary`] with an explicit dictionary.
pub async fn subscribe_events_uncompressed<W, S>(
    ws_client: &W,
    base: &Uri<String>,
    filters: &ReplayFilters,
    options: LiveOptions,
) -> Result<LiveStream<W::Error, S>, LiveError<W::Error>>
where
    W: WebSocketClient,
    S: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    <S as FromStr>::Err: std::fmt::Debug,
{
    #[cfg(feature = "zstd")]
    let dictionary = None;
    #[cfg(feature = "zstd")]
    return establish_jetstream_v2(ws_client, base, filters, options, dictionary).await;
    #[cfg(not(feature = "zstd"))]
    establish_jetstream_v2(ws_client, base, filters, options).await
}

/// Establish a compressed live connection: negotiates
/// `zstdDictionary=<id>` and decompresses each binary frame (one zstd
/// frame whose decompressed bytes are the JSON text frame) through the
/// fetched dictionary before decoding.
///
/// On an `UnknownZstdDictionary` rejection the caller refetches the
/// named dictionary and retries this function; if the fetch itself
/// fails, fall back to [`subscribe_events`].
#[cfg(feature = "zstd")]
pub async fn subscribe_events_with_dictionary<W, S>(
    ws_client: &W,
    base: &Uri<String>,
    filters: &ReplayFilters,
    options: LiveOptions,
    dictionary: &super::dictionary::ZstdDictionary,
) -> Result<LiveStream<W::Error, S>, LiveError<W::Error>>
where
    W: WebSocketClient,
    S: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    <S as FromStr>::Err: std::fmt::Debug,
{
    establish_jetstream_v2(ws_client, base, filters, options, Some(dictionary)).await
}

fn build_params<S>(
    filters: &ReplayFilters<S>,
    options: LiveOptions,
    #[cfg(feature = "zstd")] dictionary_id: Option<i64>,
) -> SubscribeEvents<S>
where
    S: BosStr + Clone + FromStr + fmt::Debug,
    <S as FromStr>::Err: std::fmt::Debug,
{
    SubscribeEvents::<S> {
        collections: (!filters.collections.is_empty()).then(|| {
            filters
                .collections
                .iter()
                .map(|c| match c {
                    CollectionFilter::Exact(nsid) => {
                        S::from_str(nsid.as_str()).expect("this better succeed")
                    }
                    CollectionFilter::Wildcard(pattern) => pattern.clone(),
                })
                .collect::<Vec<_>>()
        }),
        cursor: options.cursor,
        dids: (!filters.dids.is_empty()).then(|| filters.dids.clone()),
        kinds: (!filters.kinds.is_empty()).then(|| filters.kinds.clone()),
        max_message_size_bytes: options.max_message_size_bytes,
        zstd_dictionary: {
            #[cfg(feature = "zstd")]
            {
                dictionary_id
            }
            #[cfg(not(feature = "zstd"))]
            {
                None
            }
        },
    }
}

#[allow(clippy::too_many_arguments)]
async fn establish_jetstream_v2<W, S>(
    ws_client: &W,
    base: &Uri<String>,
    filters: &ReplayFilters,
    options: LiveOptions,
    #[cfg(feature = "zstd")] dictionary: Option<&super::dictionary::ZstdDictionary>,
) -> Result<LiveStream<W::Error, S>, LiveError<W::Error>>
where
    W: WebSocketClient,
    S: BosStr + Clone + FromStr + fmt::Debug + DeserializeOwned,
    <S as FromStr>::Err: std::fmt::Debug,
{
    #[cfg(feature = "zstd")]
    let dictionary_id = dictionary.map(|d| i64::from(d.id));
    #[cfg(feature = "zstd")]
    let max_size = match options.max_message_size_bytes {
        Some(0) | None => jacquard_common::jss::MAX_BLOCK_UNCOMPRESSED,
        Some(value) => usize::try_from(value)
            .unwrap_or(0)
            .min(jacquard_common::jss::MAX_BLOCK_UNCOMPRESSED),
    };
    #[cfg(not(feature = "zstd"))]
    let params = build_params(filters, options);
    #[cfg(feature = "zstd")]
    let params = build_params(filters, options, dictionary_id);

    let stream = ws_client
        .subscription(base.clone())
        .subscribe(&params)
        .await
        .map_err(|e| match HandshakeError::from_error(&e) {
            Some(handshake)
                if matches!(
                    handshake.error,
                    SubscribeEventsError::CursorTooOld(_)
                        | SubscribeEventsError::UnknownZstdDictionary(_)
                ) && handshake.recovery_value.is_none() =>
            {
                LiveError::MalformedRecoveryPayload(handshake)
            }
            Some(handshake) => LiveError::Handshake(handshake),
            None => LiveError::Transport(e),
        })?;

    #[cfg(feature = "zstd")]
    if let Some(dict) = dictionary {
        // Compressed path: binary frames decompress through the
        // negotiated dictionary before JSON decode.
        let dict = dict.clone();
        let (_tx, rx) = stream.into_stream_with_binary_transform(move |bytes| {
            dict.decompress_frame(bytes, max_size)
                .map(Some)
                .map_err(StreamError::decode)
        });
        return Ok(LiveStream {
            rx,
            _transport: core::marker::PhantomData,
        });
    }
    let (_tx, rx) = stream.into_stream();
    Ok(LiveStream {
        rx,
        _transport: core::marker::PhantomData,
    })
}

impl<E, S: BosStr> LiveStream<E, S> {
    /// Receive the next message. `Err(LiveError::Closed)` means the
    /// connection is over; there is no reconnection here.
    pub async fn next(&mut self) -> Result<SubscribeEventsMessage<S>, LiveError<E>> {
        use n0_future::StreamExt as _;
        match self.rx.next().await {
            Some(Ok(message)) => Ok(message),
            Some(Err(e)) => match e.kind() {
                StreamErrorKind::Decode => Err(LiveError::Decode(e)),
                StreamErrorKind::Closed => Err(LiveError::Closed),
                _ => Err(LiveError::Stream(e)),
            },
            None => Err(LiveError::Closed),
        }
    }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
    use super::*;
    use jacquard_common::stream::StreamError;
    use jacquard_common::websocket::{
        WebSocketClient, WebSocketConnectOptions, WebSocketConnection, WsMessage, WsSink, WsStream,
    };
    use smol_str::SmolStr;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

    struct NoopSink;

    impl n0_future::Sink<WsMessage> for NoopSink {
        type Error = StreamError;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, _item: WsMessage) -> Result<(), Self::Error> {
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[derive(Default)]
    struct SingleFrameClient {
        connections: Mutex<Vec<String>>,
        protocols: Mutex<Vec<String>>,
        rejection_body: Option<jacquard_common::deps::bytes::Bytes>,
        binary_frame: Option<jacquard_common::deps::bytes::Bytes>,
    }

    impl WebSocketClient for SingleFrameClient {
        type Error = jacquard_common::websocket::WebSocketError;

        async fn connect(&self, _uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
            unreachable!("subscriptions use connect_with_options")
        }

        async fn connect_with_options(
            &self,
            uri: Uri<&str>,
            options: WebSocketConnectOptions<'_>,
        ) -> Result<WebSocketConnection, Self::Error> {
            self.connections
                .lock()
                .expect("connections")
                .push(uri.as_str().to_string());
            *self.protocols.lock().expect("protocols") = options
                .protocols
                .iter()
                .map(|protocol| protocol.to_string())
                .collect();
            if let Some(body) = &self.rejection_body {
                return Err(
                    jacquard_common::websocket::WebSocketError::HandshakeRejected {
                        status: http::StatusCode::BAD_REQUEST,
                        headers: Vec::new(),
                        body: body.clone(),
                    },
                );
            }

            let frame = match &self.binary_frame {
                Some(bytes) => WsMessage::Binary(bytes.clone()),
                None => WsMessage::Text(jacquard_common::websocket::WsText::from(
                    r#"{"$type":"message","payload":{"$type":"network.bsky.jetstream.subscribeEvents#identity","did":"did:plc:test","seq":41,"time":"2026-01-01T00:00:00.000Z","identity":{"did":"did:plc:test","handle":"example.com","seq":1,"time":"2026-01-01T00:00:00.000Z"}}}"#,
                )),
            };
            let stream = n0_future::stream::iter([Ok(frame)]);
            Ok(WebSocketConnection::new(
                WsSink::new(NoopSink),
                WsStream::new(stream),
            ))
        }
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn rejected_same_dictionary_id_falls_back_uncompressed() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        struct RotatingClient {
            calls: AtomicUsize,
            uris: Mutex<Vec<String>>,
        }

        impl WebSocketClient for RotatingClient {
            type Error = jacquard_common::websocket::WebSocketError;

            async fn connect(&self, _uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
                unreachable!("subscriptions use connect_with_options")
            }

            async fn connect_with_options(
                &self,
                uri: Uri<&str>,
                _options: WebSocketConnectOptions<'_>,
            ) -> Result<WebSocketConnection, Self::Error> {
                self.uris
                    .lock()
                    .expect("uris")
                    .push(uri.as_str().to_string());
                if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(jacquard_common::websocket::WebSocketError::HandshakeRejected {
                        status: http::StatusCode::BAD_REQUEST,
                        headers: Vec::new(),
                        body: jacquard_common::deps::bytes::Bytes::from_static(
                            br#"{"error":"UnknownZstdDictionary","message":"current dictionary id is 20260811 (fetch it via getZstdDictionary and reconnect)"}"#,
                        ),
                    });
                }
                let frame = WsMessage::Text(jacquard_common::websocket::WsText::from(
                    r#"{"$type":"message","payload":{"$type":"network.bsky.jetstream.subscribeEvents#identity","did":"did:plc:test","seq":41,"time":"2026-01-01T00:00:00.000Z","identity":{"did":"did:plc:test","handle":"example.com","seq":1,"time":"2026-01-01T00:00:00.000Z"}}}"#,
                ));
                Ok(WebSocketConnection::new(
                    WsSink::new(NoopSink),
                    WsStream::new(n0_future::stream::iter([Ok(frame)])),
                ))
            }
        }

        let server = MockServer::start().await;
        let dictionary = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/jetstream/testdata/dict.bin"
        ))
        .expect("dictionary fixture");
        Mock::given(method("GET"))
            .and(path("/xrpc/network.bsky.jetstream.getZstdDictionary"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(dictionary))
            .expect(1)
            .mount(&server)
            .await;
        let base = Uri::parse(server.uri()).expect("uri");
        let archive = JetstreamClient::new(reqwest::Client::new(), base, None);
        let ws = RotatingClient {
            calls: AtomicUsize::new(0),
            uris: Mutex::new(Vec::new()),
        };

        let mut stream: LiveStream<WebSocketError, SmolStr> = subscribe_events(
            &archive,
            &ws,
            &ReplayFilters::default(),
            LiveOptions::default(),
        )
        .await
        .expect("uncompressed fallback");
        assert!(matches!(
            stream.next().await.expect("message"),
            SubscribeEventsMessage::Identity(_)
        ));
        let uris = ws.uris.lock().expect("uris");
        assert_eq!(uris.len(), 2);
        assert!(uris.iter().all(|uri| uri.starts_with("ws://")));
        assert!(uris[0].contains("zstdDictionary=20260811"));
        assert!(!uris[1].contains("zstdDictionary"));
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn compressed_low_level_stream_negotiates_dictionary_and_decodes_binary_frame() {
        use core::convert::Infallible;
        use std::io::Write as _;

        let dictionary_bytes = jacquard_common::deps::bytes::Bytes::from(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/jetstream/testdata/dict.bin"
            ))
            .expect("dictionary fixture"),
        );
        let dictionary =
            super::super::dictionary::ZstdDictionary::from_bytes::<Infallible>(dictionary_bytes)
                .expect("dictionary");
        let payload = br#"{"$type":"message","payload":{"$type":"network.bsky.jetstream.subscribeEvents#identity","did":"did:plc:test","seq":41,"time":"2026-01-01T00:00:00.000Z","identity":{"did":"did:plc:test","handle":"example.com","seq":1,"time":"2026-01-01T00:00:00.000Z"}}}"#;
        let mut encoder =
            zstd::Encoder::with_dictionary(Vec::new(), 0, &dictionary.bytes).expect("encoder");
        encoder.write_all(payload).expect("write frame");
        let client = SingleFrameClient {
            binary_frame: Some(encoder.finish().expect("compressed frame").into()),
            ..Default::default()
        };
        let base = Uri::parse("wss://jetstream.example.com")
            .expect("uri")
            .to_owned();

        let mut stream: LiveStream<WebSocketError, SmolStr> = subscribe_events_with_dictionary(
            &client,
            &base,
            &ReplayFilters::default(),
            LiveOptions::default(),
            &dictionary,
        )
        .await
        .expect("connect");
        let SubscribeEventsMessage::Identity(identity) = stream.next().await.expect("message")
        else {
            panic!("expected identity message");
        };
        assert_eq!(identity.seq, 41);
        let connections = client.connections.lock().expect("connections");
        assert_eq!(connections.len(), 1);
        assert!(connections[0].contains("zstdDictionary=20260811"));
    }

    #[cfg(feature = "zstd")]
    #[tokio::test]
    async fn compressed_stream_enforces_decompressed_message_limit() {
        use core::convert::Infallible;
        use std::io::Write as _;

        let dictionary_bytes = jacquard_common::deps::bytes::Bytes::from(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/jetstream/testdata/dict.bin"
            ))
            .expect("dictionary fixture"),
        );
        let dictionary =
            super::super::dictionary::ZstdDictionary::from_bytes::<Infallible>(dictionary_bytes)
                .expect("dictionary");
        let payload = br#"{"$type":"message","payload":{"$type":"network.bsky.jetstream.subscribeEvents#identity","did":"did:plc:test","seq":41,"time":"2026-01-01T00:00:00.000Z","identity":{"did":"did:plc:test","handle":"example.com","seq":1,"time":"2026-01-01T00:00:00.000Z"}}}"#;
        let mut encoder =
            zstd::Encoder::with_dictionary(Vec::new(), 0, &dictionary.bytes).expect("encoder");
        encoder.write_all(payload).expect("write frame");
        let client = SingleFrameClient {
            binary_frame: Some(encoder.finish().expect("compressed frame").into()),
            ..Default::default()
        };
        let base = Uri::parse("wss://jetstream.example.com")
            .expect("uri")
            .to_owned();
        let mut stream: LiveStream<WebSocketError, SmolStr> = subscribe_events_with_dictionary(
            &client,
            &base,
            &ReplayFilters::default(),
            LiveOptions {
                cursor: None,
                max_message_size_bytes: Some(8),
            },
            &dictionary,
        )
        .await
        .expect("connect");

        let LiveError::Decode(error) = stream.next().await.expect_err("oversized frame") else {
            panic!("expected decode error");
        };
        assert_eq!(error.kind(), &StreamErrorKind::Decode);
        assert_eq!(
            error.source().expect("source").to_string(),
            "frame exceeds decompression cap"
        );
    }

    #[tokio::test]
    async fn malformed_recovery_payload_is_distinct_and_preserves_body() {
        let body = jacquard_common::deps::bytes::Bytes::from_static(
            br#"{"error":"CursorTooOld","message":"the floor is unavailable"}"#,
        );
        let client = SingleFrameClient {
            rejection_body: Some(body.clone()),
            ..Default::default()
        };
        let base = Uri::parse("wss://jetstream.example.com")
            .expect("uri")
            .to_owned();

        let error = match subscribe_events_uncompressed::<_, SmolStr>(
            &client,
            &base,
            &ReplayFilters::default(),
            LiveOptions::default(),
        )
        .await
        {
            Ok(_) => panic!("malformed recovery must fail"),
            Err(error) => error,
        };
        let LiveError::MalformedRecoveryPayload(handshake) = error else {
            panic!("expected malformed recovery payload");
        };
        assert_eq!(handshake.status, 400);
        assert_eq!(handshake.body, body);
        assert!(matches!(
            handshake.error,
            SubscribeEventsError::CursorTooOld(_)
        ));
        assert_eq!(client.connections.lock().expect("connections").len(), 1);
    }

    #[tokio::test]
    async fn uncompressed_low_level_stream_omits_dictionary_and_does_not_reconnect() {
        let client = SingleFrameClient::default();
        let base = Uri::parse("wss://jetstream.example.com")
            .expect("uri")
            .to_owned();
        let mut stream: LiveStream<WebSocketError, SmolStr> = subscribe_events_uncompressed(
            &client,
            &base,
            &ReplayFilters::default(),
            LiveOptions::default(),
        )
        .await
        .expect("connect");

        let SubscribeEventsMessage::Identity(identity) = stream.next().await.expect("message")
        else {
            panic!("expected identity message");
        };
        assert_eq!(identity.seq, 41);
        assert!(matches!(stream.next().await, Err(LiveError::Closed)));

        let connections = client.connections.lock().expect("connections");
        assert_eq!(connections.len(), 1);
        assert!(!connections[0].contains("zstdDictionary"));
        assert_eq!(
            *client.protocols.lock().expect("protocols"),
            vec!["xrpc.v1.json"]
        );
    }
}
