use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::ToSmolStr;
use url::Url;

use crate::{
    CowStr, IntoStatic,
    types::{aturi::AtUri, cid::Cid, did::Did, string::AtStrError},
};

/// Generic URI with type-specific parsing
///
/// Automatically detects and parses URIs into the appropriate variant based on
/// the scheme prefix. Used in lexicon where URIs can be of various types.
///
/// Variants are checked by prefix: `did:`, `at://`, `https://`, `wss://`, `ipld://`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Uri<'u> {
    /// DID URI (did:)
    Did(Did<'u>),
    /// AT Protocol URI (at://)
    At(AtUri<'u>),
    /// HTTPS URL
    Https(Url),
    /// WebSocket Secure URL
    Wss(Url),
    /// IPLD CID URI
    Cid(Cid<'u>),
    /// Unrecognized URI scheme (catch-all)
    Any(CowStr<'u>),
}

/// Errors that can occur when parsing URIs
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum UriParseError {
    /// AT Protocol string parsing error
    #[error("Invalid atproto string: {0}")]
    At(#[from] AtStrError),
    /// Generic URL parsing error
    #[error(transparent)]
    Url(#[from] url::ParseError),
    /// CID parsing error
    #[error(transparent)]
    Cid(#[from] crate::types::cid::Error),
}

impl<'u> Uri<'u> {
    /// Parse a URI from a string slice, borrowing
    pub fn new(uri: &'u str) -> Result<Self, UriParseError> {
        if uri.starts_with("did:") {
            Ok(Uri::Did(Did::new(uri)?))
        } else if uri.starts_with("at://") {
            Ok(Uri::At(AtUri::new(uri)?))
        } else if uri.starts_with("https://") {
            Ok(Uri::Https(Url::parse(uri)?))
        } else if uri.starts_with("wss://") {
            Ok(Uri::Https(Url::parse(uri)?))
        } else if uri.starts_with("ipld://") {
            Ok(Uri::Cid(Cid::new(uri.as_bytes())?))
        } else {
            Ok(Uri::Any(CowStr::Borrowed(uri)))
        }
    }

    /// Parse a URI from a string, taking ownership
    pub fn new_owned(uri: impl AsRef<str>) -> Result<Uri<'static>, UriParseError> {
        let uri = uri.as_ref();
        if uri.starts_with("did:") {
            Ok(Uri::Did(Did::new_owned(uri)?))
        } else if uri.starts_with("at://") {
            Ok(Uri::At(AtUri::new_owned(uri)?))
        } else if uri.starts_with("https://") {
            Ok(Uri::Https(Url::parse(uri)?))
        } else if uri.starts_with("wss://") {
            Ok(Uri::Https(Url::parse(uri)?))
        } else if uri.starts_with("ipld://") {
            Ok(Uri::Cid(Cid::new_owned(uri.as_bytes())?))
        } else {
            Ok(Uri::Any(CowStr::Owned(uri.to_smolstr())))
        }
    }

    /// Get the URI as a string slice
    pub fn as_str(&self) -> &str {
        match self {
            Uri::Did(did) => did.as_str(),
            Uri::At(at_uri) => at_uri.as_str(),
            Uri::Https(url) => url.as_str(),
            Uri::Wss(url) => url.as_str(),
            Uri::Cid(cid) => cid.as_str(),
            Uri::Any(s) => s.as_ref(),
        }
    }
}

impl Serialize for Uri<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, 'a> Deserialize<'de> for Uri<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl<'s> AsRef<str> for Uri<'s> {
    fn as_ref(&self) -> &str {
        match self {
            Uri::Did(did) => did.as_str(),
            Uri::At(at_uri) => at_uri.as_str(),
            Uri::Https(url) => url.as_str(),
            Uri::Wss(url) => url.as_str(),
            Uri::Cid(cid) => cid.as_str(),
            Uri::Any(s) => s.as_ref(),
        }
    }
}

impl IntoStatic for Uri<'_> {
    type Output = Uri<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            Uri::Did(did) => Uri::Did(did.into_static()),
            Uri::At(at_uri) => Uri::At(AtUri::new_owned(at_uri.as_str()).unwrap()),
            Uri::Https(url) => Uri::Https(url),
            Uri::Wss(url) => Uri::Wss(url),
            Uri::Cid(cid) => Uri::Cid(cid.into_static()),
            Uri::Any(s) => Uri::Any(s.into_static()),
        }
    }
}
