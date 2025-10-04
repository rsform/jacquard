use miette::SourceSpan;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::{SmolStr, ToSmolStr};
use std::{str::FromStr, sync::Arc};

use crate::IntoStatic;
pub use crate::{
    CowStr,
    types::{
        aturi::AtUri,
        cid::{Cid, CidLink},
        datetime::Datetime,
        did::Did,
        handle::Handle,
        ident::AtIdentifier,
        language::Language,
        nsid::Nsid,
        recordkey::{RecordKey, Rkey},
        tid::Tid,
        uri::Uri,
    },
};

/// Polymorphic AT Protocol string value
///
/// Represents any AT Protocol string type, automatically detecting and parsing
/// into the appropriate variant. Used internally for generic value handling.
///
/// Variants are checked in order from most specific to least specific. Note that
/// record keys are intentionally NOT parsed from bare strings as the validation
/// is too permissive and would catch too many values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AtprotoStr<'s> {
    /// ISO 8601 datetime
    Datetime(Datetime),
    /// BCP 47 language tag
    Language(Language),
    /// Timestamp identifier
    Tid(Tid),
    /// Namespaced identifier
    Nsid(Nsid<'s>),
    /// Decentralized identifier
    Did(Did<'s>),
    /// Account handle
    Handle(Handle<'s>),
    /// Identifier (DID or handle)
    AtIdentifier(AtIdentifier<'s>),
    /// AT URI
    AtUri(AtUri<'s>),
    /// Generic URI
    Uri(Uri<'s>),
    /// Content identifier
    Cid(Cid<'s>),
    /// Record key
    RecordKey(RecordKey<Rkey<'s>>),
    /// Plain string (fallback)
    String(CowStr<'s>),
}

impl<'s> AtprotoStr<'s> {
    /// Borrowing constructor for bare atproto string values
    /// This is fairly exhaustive and potentially **slow**, prefer using anything
    /// that narrows down the search field quicker.
    ///
    /// Note: We don't construct record keys from bare strings in this because
    /// the type is too permissive and too many things would be classified as rkeys.
    ///
    /// Value object deserialization checks against the field names for common
    /// names (uri, cid, did, handle, createdAt, indexedAt, etc.) to improve
    /// performance of the happy path.
    pub fn new(string: &'s str) -> Self {
        // TODO: do some quick prefix checks like in Uri to drop through faster
        if let Ok(datetime) = Datetime::from_str(string) {
            Self::Datetime(datetime)
        } else if let Ok(lang) = Language::new(string) {
            Self::Language(lang)
        } else if let Ok(tid) = Tid::from_str(string) {
            Self::Tid(tid)
        } else if let Ok(did) = Did::new(string) {
            Self::Did(did)
        } else if let Ok(handle) = Handle::new(string) {
            Self::Handle(handle)
        } else if let Ok(atid) = AtIdentifier::new(string) {
            Self::AtIdentifier(atid)
        } else if let Ok(nsid) = Nsid::new(string) {
            Self::Nsid(nsid)
        } else if let Ok(aturi) = AtUri::new(string) {
            Self::AtUri(aturi)
        } else if let Ok(uri) = Uri::new(string) {
            Self::Uri(uri)
        } else if let Ok(cid) = Cid::new(string.as_bytes()) {
            Self::Cid(cid)
        } else {
            // We don't construct record keys from bare strings because the type is too permissive
            Self::String(CowStr::Borrowed(string))
        }
    }

    /// Get the string value regardless of variant
    pub fn as_str(&self) -> &str {
        match self {
            Self::Datetime(datetime) => datetime.as_str(),
            Self::Language(lang) => lang.as_str(),
            Self::Handle(handle) => handle.as_str(),
            Self::AtIdentifier(atid) => atid.as_str(),
            Self::Nsid(nsid) => nsid.as_str(),
            Self::AtUri(aturi) => aturi.as_str(),
            Self::Uri(uri) => uri.as_str(),
            Self::Cid(cid) => cid.as_str(),
            Self::Tid(tid) => tid.as_str(),
            Self::Did(did) => did.as_str(),
            Self::RecordKey(rkey) => rkey.as_ref(),
            Self::String(string) => string.as_ref(),
        }
    }
}

impl AtprotoStr<'static> {
    /// Owned constructor for bare atproto string values
    /// This is fairly exhaustive and potentially **slow**, prefer using anything
    /// that narrows down the search field quicker.
    ///
    /// Note: We don't construct record keys from bare strings in this because
    /// the type is too permissive and too many things would be classified as rkeys.
    ///
    /// Value object deserialization checks against the field names for common
    /// names (uri, cid, did, handle, createdAt, indexedAt, etc.) to improve
    /// performance of the happy path.
    pub fn new_owned(string: impl AsRef<str>) -> AtprotoStr<'static> {
        let string = string.as_ref();
        // TODO: do some quick prefix checks like in Uri to drop through faster
        if let Ok(datetime) = Datetime::from_str(string) {
            Self::Datetime(datetime)
        } else if let Ok(lang) = Language::new(string) {
            Self::Language(lang)
        } else if let Ok(tid) = Tid::from_str(string) {
            Self::Tid(tid)
        } else if let Ok(did) = Did::new_owned(string) {
            Self::Did(did)
        } else if let Ok(handle) = Handle::new_owned(string) {
            Self::Handle(handle)
        } else if let Ok(atid) = AtIdentifier::new_owned(string) {
            Self::AtIdentifier(atid)
        } else if let Ok(nsid) = Nsid::new_owned(string) {
            Self::Nsid(nsid)
        } else if let Ok(aturi) = AtUri::new_owned(string) {
            Self::AtUri(aturi)
        } else if let Ok(uri) = Uri::new_owned(string) {
            Self::Uri(uri)
        } else if let Ok(cid) = Cid::new_owned(string.as_bytes()) {
            Self::Cid(cid)
        } else {
            // We don't construct record keys from bare strings because the type is too permissive
            Self::String(CowStr::Owned(string.to_smolstr()))
        }
    }
}

impl<'s> AsRef<str> for AtprotoStr<'s> {
    fn as_ref(&self) -> &str {
        match self {
            Self::Datetime(datetime) => datetime.as_str(),
            Self::Language(lang) => lang.as_ref(),
            Self::Tid(tid) => tid.as_ref(),
            Self::Did(did) => did.as_ref(),
            Self::Handle(handle) => handle.as_ref(),
            Self::AtIdentifier(atid) => atid.as_ref(),
            Self::Nsid(nsid) => nsid.as_ref(),
            Self::AtUri(aturi) => aturi.as_ref(),
            Self::Uri(uri) => uri.as_str(),
            Self::Cid(cid) => cid.as_ref(),
            Self::RecordKey(rkey) => rkey.as_ref(),
            Self::String(string) => string.as_ref(),
        }
    }
}

impl Serialize for AtprotoStr<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

impl<'de, 'a> Deserialize<'de> for AtprotoStr<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Ok(Self::new(value))
    }
}

impl IntoStatic for AtprotoStr<'_> {
    type Output = AtprotoStr<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            AtprotoStr::Datetime(datetime) => AtprotoStr::Datetime(datetime),
            AtprotoStr::Language(language) => AtprotoStr::Language(language),
            AtprotoStr::Tid(tid) => AtprotoStr::Tid(tid),
            AtprotoStr::Nsid(nsid) => AtprotoStr::Nsid(nsid.into_static()),
            AtprotoStr::Did(did) => AtprotoStr::Did(did.into_static()),
            AtprotoStr::Handle(handle) => AtprotoStr::Handle(handle.into_static()),
            AtprotoStr::AtIdentifier(ident) => AtprotoStr::AtIdentifier(ident.into_static()),
            AtprotoStr::AtUri(at_uri) => AtprotoStr::AtUri(at_uri.into_static()),
            AtprotoStr::Uri(uri) => AtprotoStr::Uri(uri.into_static()),
            AtprotoStr::Cid(cid) => AtprotoStr::Cid(cid.into_static()),
            AtprotoStr::RecordKey(record_key) => AtprotoStr::RecordKey(record_key.into_static()),
            AtprotoStr::String(cow_str) => AtprotoStr::String(cow_str.into_static()),
        }
    }
}

impl From<AtprotoStr<'_>> for String {
    fn from(value: AtprotoStr<'_>) -> Self {
        match value {
            AtprotoStr::AtIdentifier(ident) => ident.to_string(),
            AtprotoStr::AtUri(at_uri) => at_uri.to_string(),
            AtprotoStr::Uri(uri) => match uri {
                Uri::At(at_uri) => at_uri.to_string(),
                Uri::Cid(cid) => cid.to_string(),
                Uri::Did(did) => did.to_string(),
                Uri::Https(url) => url.to_string(),
                Uri::Wss(url) => url.to_string(),
                Uri::Any(cow_str) => cow_str.to_string(),
            },
            AtprotoStr::Cid(cid) => cid.to_string(),
            AtprotoStr::RecordKey(record_key) => record_key.as_ref().to_string(),
            AtprotoStr::String(cow_str) => cow_str.to_string(),
            AtprotoStr::Datetime(datetime) => datetime.to_string(),
            AtprotoStr::Language(language) => language.to_string(),
            AtprotoStr::Tid(tid) => tid.to_string(),
            AtprotoStr::Nsid(nsid) => nsid.to_string(),
            AtprotoStr::Did(did) => did.to_string(),
            AtprotoStr::Handle(handle) => handle.to_string(),
        }
    }
}

/// Parsing Error for atproto string types which don't have third-party specs
/// (e.g. datetime, CIDs, language tags).
///
/// `spec` refers to the final url path segment on atproto.com/specs,
/// detailing the specification for the type
/// `source` is the source string, or part of it
/// `kind` is the type of parsing error: `[StrParseKind]`
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("error in `{source}`: {kind}")]
#[diagnostic(
    url("https://atproto.com/specs/{spec}"),
    help("if something doesn't match the spec, contact the crate author")
)]
pub struct AtStrError {
    /// AT Protocol spec name this error relates to
    pub spec: SmolStr,
    /// The source string that failed to parse
    #[source_code]
    pub source: String,
    /// The specific kind of parsing error
    #[source]
    #[diagnostic_source]
    pub kind: StrParseKind,
}

impl AtStrError {
    /// Create a new AT string parsing error
    pub fn new(spec: &'static str, source: String, kind: StrParseKind) -> Self {
        Self {
            spec: SmolStr::new_static(spec),
            source,
            kind,
        }
    }

    /// Wrap an existing error with a new spec context
    pub fn wrap(spec: &'static str, source: String, error: AtStrError) -> Self {
        if let Some(span) = match &error.kind {
            StrParseKind::Disallowed { problem, .. } => problem,
            StrParseKind::MissingComponent { span, .. } => span,
            _ => &None,
        } {
            Self {
                spec: SmolStr::new_static(spec),
                source,
                kind: StrParseKind::Wrap {
                    span: Some(*span),
                    err: Arc::new(error),
                },
            }
        } else {
            let span = source
                .find(&error.source)
                .map(|start| (start, error.source.len()).into());
            Self {
                spec: SmolStr::new_static(spec),
                source,
                kind: StrParseKind::Wrap {
                    span,
                    err: Arc::new(error),
                },
            }
        }
    }

    /// something on the provided disallowed list was found in the source string
    /// does a substring search for any of the offending strings to get the span
    pub fn disallowed(spec: &'static str, source: &str, disallowed: &[&str]) -> Self {
        for item in disallowed {
            if let Some(loc) = source.find(item) {
                return Self {
                    spec: SmolStr::new_static(spec),
                    source: source.to_string(),
                    kind: StrParseKind::Disallowed {
                        problem: Some(SourceSpan::new(loc.into(), item.len())),
                        message: smol_str::format_smolstr!("`{item}`"),
                    },
                };
            }
        }
        Self {
            spec: SmolStr::new_static(spec),
            source: source.to_string(),
            kind: StrParseKind::Disallowed {
                problem: None,
                message: SmolStr::new_static(""),
            },
        }
    }

    /// Create an error for a string that exceeds the maximum length
    pub fn too_long(spec: &'static str, source: &str, max: usize, actual: usize) -> Self {
        Self {
            spec: SmolStr::new_static(spec),
            source: source.to_string(),
            kind: StrParseKind::TooLong { max, actual },
        }
    }

    /// Create an error for a string below the minimum length
    pub fn too_short(spec: &'static str, source: &str, min: usize, actual: usize) -> Self {
        Self {
            spec: SmolStr::new_static(spec),
            source: source.to_string(),
            kind: StrParseKind::TooShort { min, actual },
        }
    }

    /// missing component, with what was expected to be found
    pub fn missing(spec: &'static str, source: &str, expected: &str) -> Self {
        if let Some(loc) = source.find(expected) {
            return Self {
                spec: SmolStr::new_static(spec),
                source: source.to_string(),
                kind: StrParseKind::MissingComponent {
                    span: Some(SourceSpan::new(loc.into(), expected.len())),
                    message: smol_str::format_smolstr!("`{expected}` found incorrectly here"),
                },
            };
        }
        Self {
            spec: SmolStr::new_static(spec),
            source: source.to_string(),
            kind: StrParseKind::MissingComponent {
                span: None,
                message: SmolStr::new(expected),
            },
        }
    }

    /// missing component, with the span where it was expected to be founf
    /// Create an error for a missing component at a specific span
    pub fn missing_from(
        spec: &'static str,
        source: &str,
        expected: &str,
        span: (usize, usize),
    ) -> Self {
        Self {
            spec: SmolStr::new_static(spec),
            source: source.to_string(),
            kind: StrParseKind::MissingComponent {
                span: Some(span.into()),
                message: SmolStr::new(expected),
            },
        }
    }

    /// Create an error for a regex validation failure
    pub fn regex(spec: &'static str, source: &str, message: SmolStr) -> Self {
        Self {
            spec: SmolStr::new_static(spec),
            source: source.to_string(),
            kind: StrParseKind::RegexFail {
                span: None,
                message,
            },
        }
    }
}

/// Kinds of parsing errors for AT Protocol string types
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum StrParseKind {
    /// Regex pattern validation failed
    #[error("regex failure - {message}")]
    #[diagnostic(code(jacquard::types::string::regex_fail))]
    RegexFail {
        /// Optional span highlighting the problem area
        #[label]
        span: Option<SourceSpan>,
        /// Help message explaining the failure
        #[help]
        message: SmolStr,
    },
    /// String exceeds maximum allowed length
    #[error("string too long (allowed: {max}, actual: {actual})")]
    #[diagnostic(code(jacquard::types::string::wrong_length))]
    TooLong {
        /// Maximum allowed length
        max: usize,
        /// Actual string length
        actual: usize,
    },

    /// String is below minimum required length
    #[error("string too short (allowed: {min}, actual: {actual})")]
    #[diagnostic(code(jacquard::types::string::wrong_length))]
    TooShort {
        /// Minimum required length
        min: usize,
        /// Actual string length
        actual: usize,
    },
    /// String contains disallowed values
    #[error("disallowed - {message}")]
    #[diagnostic(code(jacquard::types::string::disallowed))]
    Disallowed {
        /// Optional span highlighting the disallowed content
        #[label]
        problem: Option<SourceSpan>,
        /// Help message about what's disallowed
        #[help]
        message: SmolStr,
    },
    /// Required component is missing
    #[error("missing - {message}")]
    #[diagnostic(code(jacquard::atstr::missing_component))]
    MissingComponent {
        /// Optional span where the component should be
        #[label]
        span: Option<SourceSpan>,
        /// Help message about what's missing
        #[help]
        message: SmolStr,
    },
    /// Wraps another error with additional context
    #[error("{err:?}")]
    #[diagnostic(code(jacquard::atstr::inner))]
    Wrap {
        /// Optional span in the outer context
        #[label]
        span: Option<SourceSpan>,
        /// The wrapped inner error
        #[source]
        err: Arc<AtStrError>,
    },
}
