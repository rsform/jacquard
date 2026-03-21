use crate::cowstr::ToCowStr;
use crate::deps::fluent_uri::Uri;
use crate::{
    CowStr, IntoStatic,
    types::{
        aturi::AtUri, cid::Cid, collection::Collection, did::Did, nsid::Nsid, string::AtStrError,
    },
};
use alloc::string::String;
use core::{fmt::Display, marker::PhantomData, ops::Deref, str::FromStr};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::ToSmolStr;

/// Generic URI with type-specific parsing
///
/// Automatically detects and parses URIs into the appropriate variant based on
/// the scheme prefix. Used in lexicon where URIs can be of various types.
///
/// Variants are checked by prefix: `did:`, `at://`, `https://`, `wss://`, `ipld://`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UriValue<'u> {
    /// DID URI (did:)
    Did(Did<CowStr<'u>>),
    /// AT Protocol URI (at://)
    At(AtUri<'u>),
    /// HTTPS URL
    Https(Uri<String>),
    /// WebSocket Secure URL
    Wss(Uri<String>),
    /// IPLD CID URI
    Cid(Cid<'u>),
    /// Unrecognized URI scheme (catch-all)
    Any(CowStr<'u>),
}

/// Errors that can occur when parsing URIs
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum UriParseError {
    /// AT Protocol string parsing error
    #[error("Invalid atproto string: {0}")]
    At(#[from] AtStrError),
    /// URI parsing error
    #[error(transparent)]
    Uri(#[from] crate::deps::fluent_uri::ParseError),
    /// CID parsing error
    #[error(transparent)]
    Cid(#[from] crate::types::cid::Error),
}

impl<'u> UriValue<'u> {
    /// Parse a URI from a string slice, borrowing
    pub fn new(uri: &'u str) -> Result<Self, UriParseError> {
        if uri.starts_with("did:") {
            Ok(UriValue::Did(Did::new_cow(uri.to_cowstr())?))
        } else if uri.starts_with("at://") {
            Ok(UriValue::At(AtUri::new(uri)?))
        } else if uri.starts_with("https://") {
            Ok(UriValue::Https(Uri::parse(uri)?.to_owned()))
        } else if uri.starts_with("wss://") {
            Ok(UriValue::Wss(Uri::parse(uri)?.to_owned()))
        } else if uri.starts_with("ipld://") {
            match Cid::from_str(&uri[7..]) {
                Ok(cid) => Ok(UriValue::Cid(cid)),
                Err(_) => Ok(UriValue::Any(CowStr::Borrowed(uri))),
            }
        } else {
            Ok(UriValue::Any(CowStr::Borrowed(uri)))
        }
    }

    /// Parse a URI from a string, taking ownership
    pub fn new_owned(uri: impl AsRef<str>) -> Result<UriValue<'static>, UriParseError> {
        let uri = uri.as_ref();
        if uri.starts_with("did:") {
            Ok(UriValue::Did(Did::new_owned(uri)?))
        } else if uri.starts_with("at://") {
            Ok(UriValue::At(AtUri::new_owned(uri)?))
        } else if uri.starts_with("https://") {
            Ok(UriValue::Https(Uri::parse(uri)?.to_owned()))
        } else if uri.starts_with("wss://") {
            Ok(UriValue::Wss(Uri::parse(uri)?.to_owned()))
        } else if uri.starts_with("ipld://") {
            match Cid::from_str(&uri[7..]) {
                Ok(cid) => Ok(UriValue::Cid(cid)),
                Err(_) => Ok(UriValue::Any(CowStr::Owned(uri.to_smolstr()))),
            }
        } else {
            Ok(UriValue::Any(CowStr::Owned(uri.to_smolstr())))
        }
    }

    /// Parse a URI from a CowStr, borrowing where possible
    pub fn new_cow(uri: CowStr<'u>) -> Result<Self, UriParseError> {
        if uri.starts_with("did:") {
            Ok(UriValue::Did(Did::new_cow(uri)?))
        } else if uri.starts_with("at://") {
            Ok(UriValue::At(AtUri::new_cow(uri)?))
        } else if uri.starts_with("https://") {
            Ok(UriValue::Https(Uri::parse(uri.as_ref())?.to_owned()))
        } else if uri.starts_with("wss://") {
            Ok(UriValue::Wss(Uri::parse(uri.as_ref())?.to_owned()))
        } else if uri.starts_with("ipld://") {
            match Cid::from_str(&uri.as_str()[7..]) {
                Ok(cid) => Ok(UriValue::Cid(cid)),
                Err(_) => Ok(UriValue::Any(uri)),
            }
        } else {
            Ok(UriValue::Any(uri))
        }
    }

    /// Get the URI as a string slice
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

impl Serialize for UriValue<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, 'a> Deserialize<'de> for UriValue<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = Deserialize::deserialize(deserializer)?;
        Self::new_cow(value).map_err(D::Error::custom)
    }
}

impl<'s> AsRef<str> for UriValue<'s> {
    fn as_ref(&self) -> &str {
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

impl IntoStatic for UriValue<'_> {
    type Output = UriValue<'static>;

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

#[repr(transparent)]
/// Collection type-annotated at:// URI
///
/// Carries the corresponding collection type for fetching records easily
pub struct RecordUri<'a, R: Collection>(AtUri<'a>, PhantomData<R>);

impl<'a, R: Collection> RecordUri<'a, R> {
    /// attepts to parse an at-uri as the corresponding collection
    pub fn try_from_uri(uri: AtUri<'a>) -> Result<Self, UriError> {
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

    /// Spits out the internal un-typed AtUri
    pub fn into_inner(self) -> AtUri<'a> {
        self.0
    }

    /// Accesses the internal AtUri for use
    pub fn as_uri(&self) -> &AtUri<'a> {
        &self.0
    }
}

impl<R: Collection> Display for RecordUri<'_, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a, R: Collection> AsRef<AtUri<'a>> for RecordUri<'a, R> {
    fn as_ref(&self) -> &AtUri<'a> {
        &self.0
    }
}

impl<'a, R: Collection> Deref for RecordUri<'a, R> {
    type Target = AtUri<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
/// Errors that can occur when parsing or validating collection type-annotated URIs
pub enum UriError {
    /// Given at-uri didn't have the matching collection for the record
    #[error("Collection mismatch: expected {expected}, found {found:?}")]
    CollectionMismatch {
        /// The collection of the record
        expected: &'static str,
        /// What the at-uri had
        found: Option<Nsid>,
    },
    /// Couldn't parse the string as an AtUri
    #[error("Invalid URI: {0}")]
    InvalidUri(#[from] AtStrError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wss_variant_parsing() {
        // Test that wss:// URIs are parsed as UriValue::Wss, not UriValue::Https
        let uri = UriValue::new("wss://example.com/path").expect("valid wss uri");
        assert!(
            matches!(uri, UriValue::Wss(_)),
            "wss:// should parse to UriValue::Wss"
        );
        assert_eq!(uri.as_str(), "wss://example.com/path");
    }

    #[test]
    fn test_https_variant_parsing() {
        // Test that https:// URIs are parsed as UriValue::Https
        let uri = UriValue::new("https://example.com/path").expect("valid https uri");
        assert!(
            matches!(uri, UriValue::Https(_)),
            "https:// should parse to UriValue::Https"
        );
        assert_eq!(uri.as_str(), "https://example.com/path");
    }

    #[test]
    fn test_wss_owned_variant_parsing() {
        // Test that owned wss:// parsing works correctly
        let uri = UriValue::new_owned("wss://example.com").expect("valid wss uri");
        assert!(
            matches!(uri, UriValue::Wss(_)),
            "owned wss:// should parse to UriValue::Wss"
        );
        assert_eq!(uri.as_str(), "wss://example.com");
    }

    #[test]
    fn test_https_owned_variant_parsing() {
        // Test that owned https:// parsing works correctly
        let uri = UriValue::new_owned("https://example.com").expect("valid https uri");
        assert!(
            matches!(uri, UriValue::Https(_)),
            "owned https:// should parse to UriValue::Https"
        );
        assert_eq!(uri.as_str(), "https://example.com");
    }

    #[test]
    fn test_wss_cow_variant_parsing() {
        // Test that cow variant parsing works correctly for wss://
        let uri = UriValue::new_cow(CowStr::Borrowed("wss://example.com")).expect("valid wss uri");
        assert!(
            matches!(uri, UriValue::Wss(_)),
            "cow wss:// should parse to UriValue::Wss"
        );
        assert_eq!(uri.as_str(), "wss://example.com");
    }

    #[test]
    fn test_https_cow_variant_parsing() {
        // Test that cow variant parsing works correctly for https://
        let uri =
            UriValue::new_cow(CowStr::Borrowed("https://example.com")).expect("valid https uri");
        assert!(
            matches!(uri, UriValue::Https(_)),
            "cow https:// should parse to UriValue::Https"
        );
        assert_eq!(uri.as_str(), "https://example.com");
    }

    #[test]
    fn test_uri_display() {
        // Test that Display output preserves the original scheme
        let wss = UriValue::new_owned("wss://example.com").unwrap();
        assert_eq!(wss.as_str(), "wss://example.com");

        let https = UriValue::new_owned("https://example.com").unwrap();
        assert_eq!(https.as_str(), "https://example.com");
    }

    #[test]
    fn test_into_static_preserves_variant() {
        // Test that IntoStatic conversion preserves the variant type
        let wss = UriValue::new_owned("wss://example.com").unwrap();
        let static_wss = wss.into_static();
        assert!(matches!(static_wss, UriValue::Wss(_)));

        let https = UriValue::new_owned("https://example.com").unwrap();
        let static_https = https.into_static();
        assert!(matches!(static_https, UriValue::Https(_)));
    }
}
