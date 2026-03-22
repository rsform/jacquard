use crate::bos::Bos;
use crate::deps::fluent_uri::Uri;
use crate::{
    CowStr, DefaultStr, IntoStatic,
    types::{
        aturi::{AtUri, validate_and_index},
        cid::Cid,
        collection::Collection,
        did::{Did, validate_did},
        nsid::Nsid,
        string::{AtStrError, StrParseKind},
    },
};
use alloc::string::{String, ToString};
use core::{fmt::Display, marker::PhantomData, ops::Deref, str::FromStr};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::{SmolStr, ToSmolStr};

/// Generic URI with type-specific parsing.
///
/// Automatically detects and parses URIs into the appropriate variant based on
/// the scheme prefix. Used in lexicon where URIs can be of various types.
///
/// Variants are checked by prefix: `did:`, `at://`, `https://`, `wss://`, `ipld://`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UriValue<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// DID URI (did:).
    Did(Did<S>),
    /// AT Protocol URI (at://).
    At(AtUri<S>),
    /// HTTPS URL.
    Https(Uri<String>),
    /// WebSocket Secure URL.
    Wss(Uri<String>),
    /// IPLD CID URI.
    Cid(Cid<S>),
    /// Unrecognized URI scheme (catch-all).
    Any(S),
}

/// Errors that can occur when parsing URIs.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum UriParseError {
    /// AT Protocol string parsing error.
    #[error("Invalid atproto string: {0}")]
    At(#[from] AtStrError),
    /// URI parsing error.
    #[error(transparent)]
    Uri(#[from] crate::deps::fluent_uri::ParseError),
    /// CID parsing error.
    #[error(transparent)]
    Cid(#[from] crate::types::cid::Error),
}

// ---------------------------------------------------------------------------
// Generic construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> UriValue<S> {
    /// Parse a URI, validate by prefix, wrap `S` into the matching variant.
    ///
    /// `Https` and `Wss` variants always allocate a `Uri<String>` regardless of `S`.
    pub fn new(uri: S) -> Result<Self, UriParseError> {
        let s = uri.as_ref();
        if s.starts_with("did:") {
            if validate_did(s).is_ok() {
                return Ok(UriValue::Did(unsafe { Did::unchecked(uri) }));
            }
        } else if s.starts_with("at://") {
            if let Ok(indices) = validate_and_index(s) {
                return Ok(UriValue::At(unsafe { AtUri::from_parts(uri, indices) }));
            }
        } else if s.starts_with("https://") {
            if let Ok(parsed) = Uri::parse(s) {
                return Ok(UriValue::Https(parsed.to_owned()));
            }
        } else if s.starts_with("wss://") {
            if let Ok(parsed) = Uri::parse(s) {
                return Ok(UriValue::Wss(parsed.to_owned()));
            }
        } else if s.starts_with("ipld://") {
            return Ok(UriValue::Cid(unsafe { Cid::unchecked_str(uri) }));
        }
        Ok(UriValue::Any(uri))
    }
}

// ---------------------------------------------------------------------------
// Owned construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str> + FromStr> UriValue<S> {
    /// Parse a URI from a string, taking ownership.
    pub fn new_owned(uri: impl AsRef<str>) -> Result<Self, UriParseError> {
        let uri_str = uri.as_ref();
        if uri_str.starts_with("did:") {
            Ok(UriValue::Did(Did::new_owned(uri_str)?))
        } else if uri_str.starts_with("at://") {
            Ok(UriValue::At(AtUri::new_owned(uri_str)?))
        } else if uri_str.starts_with("https://") {
            Ok(UriValue::Https(Uri::parse(uri_str)?.to_owned()))
        } else if uri_str.starts_with("wss://") {
            Ok(UriValue::Wss(Uri::parse(uri_str)?.to_owned()))
        } else if uri_str.starts_with("ipld://") {
            let cid_part = &uri_str[7..];
            if cid_part.is_empty() {
                let s = S::from_str(uri_str).map_err(|_| {
                    UriParseError::At(AtStrError::new(
                        "uri",
                        uri_str.to_string(),
                        StrParseKind::Conversion,
                    ))
                })?;
                Ok(UriValue::Any(s))
            } else {
                let s = S::from_str(cid_part).map_err(|_| {
                    UriParseError::At(AtStrError::new(
                        "uri",
                        cid_part.to_string(),
                        StrParseKind::Conversion,
                    ))
                })?;
                Ok(UriValue::Cid(unsafe { Cid::unchecked_str(s) }))
            }
        } else {
            let s = S::from_str(uri_str).map_err(|_| {
                UriParseError::At(AtStrError::new(
                    "uri",
                    uri_str.to_string(),
                    StrParseKind::Conversion,
                ))
            })?;
            Ok(UriValue::Any(s))
        }
    }
}

// ---------------------------------------------------------------------------
// Accessors
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> UriValue<S> {
    /// Get the URI as a string slice.
    pub fn as_str(&self) -> &str {
        match self {
            UriValue::Did(did) => did.as_str(),
            UriValue::At(at_uri) => at_uri.as_str(),
            UriValue::Https(url) => url.as_str(),
            UriValue::Wss(url) => url.as_str(),
            UriValue::Cid(cid) => cid.as_str(),
            UriValue::Any(s) => s.as_ref(),
        }
    }
}

// ---------------------------------------------------------------------------
// Serde
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Serialize for UriValue<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UriValue<CowStr<'de>> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: CowStr<'de> = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for UriValue<SmolStr> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: SmolStr = Deserialize::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// AsRef, IntoStatic
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> AsRef<str> for UriValue<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for UriValue<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = UriValue<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            UriValue::Did(did) => UriValue::Did(did.into_static()),
            UriValue::At(at_uri) => UriValue::At(at_uri.into_static()),
            UriValue::Https(url) => UriValue::Https(url),
            UriValue::Wss(url) => UriValue::Wss(url),
            UriValue::Cid(cid) => UriValue::Cid(cid.into_static()),
            UriValue::Any(s) => UriValue::Any(s.into_static()),
        }
    }
}

// ---------------------------------------------------------------------------
// RecordUri
// ---------------------------------------------------------------------------

#[repr(transparent)]
/// Collection type-annotated at:// URI.
///
/// Carries the corresponding collection type for fetching records easily.
pub struct RecordUri<S: Bos<str> + AsRef<str>, R: Collection>(AtUri<S>, PhantomData<R>);

impl<S: Bos<str> + AsRef<str>, R: Collection> RecordUri<S, R> {
    /// Attempts to parse an at-uri as the corresponding collection.
    pub fn try_from_uri(uri: AtUri<S>) -> Result<Self, UriError> {
        if let Some(collection) = uri.collection() {
            if collection.as_str() == R::NSID {
                return Ok(Self(uri, PhantomData));
            }
        }
        Err(UriError::CollectionMismatch {
            expected: R::NSID,
            found: uri
                .collection()
                .map(|c| Nsid::new_owned(c.as_str()).unwrap()),
        })
    }

    /// Returns the internal un-typed AtUri.
    pub fn into_inner(self) -> AtUri<S> {
        self.0
    }

    /// Accesses the internal AtUri for use.
    pub fn as_uri(&self) -> &AtUri<S> {
        &self.0
    }
}

impl<S: Bos<str> + AsRef<str>, R: Collection> Display for RecordUri<S, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl<S: Bos<str> + AsRef<str>, R: Collection> AsRef<AtUri<S>> for RecordUri<S, R> {
    fn as_ref(&self) -> &AtUri<S> {
        &self.0
    }
}

impl<S: Bos<str> + AsRef<str>, R: Collection> Deref for RecordUri<S, R> {
    type Target = AtUri<S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
/// Errors that can occur when parsing or validating collection type-annotated URIs.
pub enum UriError {
    /// Given at-uri didn't have the matching collection for the record.
    #[error("Collection mismatch: expected {expected}, found {found:?}")]
    CollectionMismatch {
        /// The collection of the record.
        expected: &'static str,
        /// What the at-uri had.
        found: Option<Nsid>,
    },
    /// Couldn't parse the string as an AtUri.
    #[error("Invalid URI: {0}")]
    InvalidUri(#[from] AtStrError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wss_variant_parsing() {
        let uri = UriValue::new("wss://example.com/path").expect("valid wss uri");
        assert!(
            matches!(uri, UriValue::Wss(_)),
            "wss:// should parse to UriValue::Wss"
        );
        assert_eq!(uri.as_str(), "wss://example.com/path");
    }

    #[test]
    fn test_https_variant_parsing() {
        let uri = UriValue::new("https://example.com/path").expect("valid https uri");
        assert!(
            matches!(uri, UriValue::Https(_)),
            "https:// should parse to UriValue::Https"
        );
        assert_eq!(uri.as_str(), "https://example.com/path");
    }

    #[test]
    fn test_wss_owned_variant_parsing() {
        let uri: UriValue<SmolStr> =
            UriValue::new_owned("wss://example.com").expect("valid wss uri");
        assert!(
            matches!(uri, UriValue::Wss(_)),
            "owned wss:// should parse to UriValue::Wss"
        );
        assert_eq!(uri.as_str(), "wss://example.com");
    }

    #[test]
    fn test_https_owned_variant_parsing() {
        let uri: UriValue<SmolStr> =
            UriValue::new_owned("https://example.com").expect("valid https uri");
        assert!(
            matches!(uri, UriValue::Https(_)),
            "owned https:// should parse to UriValue::Https"
        );
        assert_eq!(uri.as_str(), "https://example.com");
    }

    #[test]
    fn test_wss_cow_variant_parsing() {
        let uri = UriValue::new(CowStr::Borrowed("wss://example.com")).expect("valid wss uri");
        assert!(
            matches!(uri, UriValue::Wss(_)),
            "cow wss:// should parse to UriValue::Wss"
        );
        assert_eq!(uri.as_str(), "wss://example.com");
    }

    #[test]
    fn test_https_cow_variant_parsing() {
        let uri = UriValue::new(CowStr::Borrowed("https://example.com")).expect("valid https uri");
        assert!(
            matches!(uri, UriValue::Https(_)),
            "cow https:// should parse to UriValue::Https"
        );
        assert_eq!(uri.as_str(), "https://example.com");
    }

    #[test]
    fn test_uri_display() {
        let wss: UriValue<SmolStr> = UriValue::new_owned("wss://example.com").unwrap();
        assert_eq!(wss.as_str(), "wss://example.com");

        let https: UriValue<SmolStr> = UriValue::new_owned("https://example.com").unwrap();
        assert_eq!(https.as_str(), "https://example.com");
    }

    #[test]
    fn test_into_static_preserves_variant() {
        let wss: UriValue<SmolStr> = UriValue::new_owned("wss://example.com").unwrap();
        let static_wss = wss.into_static();
        assert!(matches!(static_wss, UriValue::Wss(_)));

        let https: UriValue<SmolStr> = UriValue::new_owned("https://example.com").unwrap();
        let static_https = https.into_static();
        assert!(matches!(static_https, UriValue::Https(_)));
    }
}
