use crate::{
    CowStr, IntoStatic,
    types::{
        aturi::AtUri, cid::Cid, collection::Collection, did::Did, nsid::Nsid, string::AtStrError,
    },
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::ToSmolStr;
use std::{fmt::Display, marker::PhantomData, ops::Deref, str::FromStr};
use url::Url;

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
            Ok(Uri::Cid(
                Cid::from_str(uri.strip_prefix("ipld://").unwrap_or(uri.as_ref())).unwrap(),
            ))
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
            Ok(Uri::Cid(
                Cid::from_str(uri.strip_prefix("ipld://").unwrap_or(uri.as_ref())).unwrap(),
            ))
        } else {
            Ok(Uri::Any(CowStr::Owned(uri.to_smolstr())))
        }
    }

    /// Parse a URI from a CowStr, borrowing where possible
    pub fn new_cow(uri: CowStr<'u>) -> Result<Self, UriParseError> {
        if uri.starts_with("did:") {
            Ok(Uri::Did(Did::new_cow(uri)?))
        } else if uri.starts_with("at://") {
            Ok(Uri::At(AtUri::new_cow(uri)?))
        } else if uri.starts_with("https://") {
            Ok(Uri::Https(Url::parse(uri.as_ref())?))
        } else if uri.starts_with("wss://") {
            Ok(Uri::Https(Url::parse(uri.as_ref())?))
        } else if uri.starts_with("ipld://") {
            Ok(Uri::Cid(
                Cid::from_str(uri.strip_prefix("ipld://").unwrap_or(uri.as_str())).unwrap(),
            ))
        } else {
            Ok(Uri::Any(uri))
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
        Self::new_cow(value).map_err(D::Error::custom)
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
            found: uri.collection().map(|c| c.clone().into_static()),
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
/// Errors that can occur when parsing or validating collection type-annotated URIs
pub enum UriError {
    /// Given at-uri didn't have the matching collection for the record
    #[error("Collection mismatch: expected {expected}, found {found:?}")]
    CollectionMismatch {
        /// The collection of the record
        expected: &'static str,
        /// What the at-uri had
        found: Option<Nsid<'static>>,
    },
    /// Couldn't parse the string as an AtUri
    #[error("Invalid URI: {0}")]
    InvalidUri(#[from] AtStrError),
}
