use crate::bos::{Bos, DefaultStr};
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::str::FromStr;
#[cfg(feature = "std")]
use miette::{Diagnostic, SourceSpan};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

/// Source span for error reporting (offset, length)
/// With `std` feature, this is `miette::SourceSpan`. Without, a simple tuple struct.
#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan(pub usize, pub usize);

#[cfg(not(feature = "std"))]
impl SourceSpan {
    pub fn new(offset: usize, len: usize) -> Self {
        Self(offset, len)
    }
}

#[cfg(not(feature = "std"))]
impl From<(usize, usize)> for SourceSpan {
    fn from((offset, len): (usize, usize)) -> Self {
        Self(offset, len)
    }
}

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
        uri::UriValue,
    },
};
use crate::{
    IntoStatic,
    types::{LexiconStringType, UriType},
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
pub enum AtprotoStr<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// ISO 8601 datetime
    Datetime(Datetime),
    /// BCP 47 language tag
    Language(Language),
    /// Timestamp identifier
    Tid(Tid),
    /// Namespaced identifier
    Nsid(Nsid<S>),
    /// Decentralized identifier
    Did(Did<S>),
    /// Account handle
    Handle(Handle<S>),
    /// Identifier (DID or handle)
    AtIdentifier(AtIdentifier<S>),
    /// AT URI
    AtUri(AtUri<S>),
    /// Generic URI
    Uri(UriValue<S>),
    /// Content identifier
    Cid(Cid<S>),
    /// Record key
    RecordKey(RecordKey<Rkey<S>>),
    /// Plain string (fallback)
    String(S),
}

use crate::types::cid::IpldCid;
use crate::types::did::validate_did;
use crate::types::handle::validate_handle;
use crate::types::nsid::validate_nsid;

impl<S: Bos<str> + AsRef<str>> AtprotoStr<S> {
    /// Classify and wrap a string value into the appropriate variant.
    ///
    /// This is fairly exhaustive and potentially **slow**, prefer using anything
    /// that narrows down the search field quicker.
    ///
    /// Inspects the string content, validates against known AT Protocol types,
    /// and moves `string` into the matching variant via unchecked constructors
    /// (safe because we validate first).
    pub fn new(string: S) -> Self {
        let s: &str = string.as_ref();
        // Non-string-backed types first (they don't consume S).
        if let Ok(datetime) = Datetime::from_str(s) {
            return Self::Datetime(datetime);
        }
        if let Ok(lang) = Language::new(s) {
            return Self::Language(lang);
        }
        if let Ok(tid) = Tid::from_str(s) {
            return Self::Tid(tid);
        }
        // String-backed types: validate then wrap S directly.
        if validate_did(s).is_ok() {
            return Self::Did(unsafe { Did::unchecked(string) });
        }
        if validate_handle(s).is_ok() {
            return Self::Handle(unsafe { Handle::unchecked(string) });
        }
        if validate_nsid(s).is_ok() {
            return Self::Nsid(unsafe { Nsid::unchecked(string) });
        }
        if crate::types::aturi::validate_and_index(s).is_ok() {
            return Self::AtUri(unsafe { AtUri::unchecked(string) });
        }
        // URI schemes that UriValue handles - check prefix, wrap S directly.
        if s.starts_with("https://") || s.starts_with("wss://") || s.starts_with("ipld://") {
            if let Ok(uri) = UriValue::new(s) {
                // we don't want to always Any here, it's better to fall back to the String variant.
                match uri {
                    UriValue::Any(_) => {}
                    _ => {
                        return Self::Uri(UriValue::new(string).expect("already checked"));
                    }
                }
            }
        }
        let s: &str = string.as_ref();
        // CID: try to parse as IPLD first, otherwise wrap as string CID.
        if IpldCid::try_from(s).is_ok() || s.starts_with("bafy") {
            return Self::Cid(unsafe { Cid::unchecked_str(string) });
        }
        // Fallback: plain string.
        Self::String(string)
    }

    /// Get the string value regardless of variant.
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

    /// Detailed string type classification.
    pub fn string_type(&self) -> LexiconStringType {
        match self {
            Self::Datetime(_) => LexiconStringType::Datetime,
            Self::Language(_) => LexiconStringType::Language,
            Self::Handle(_) => LexiconStringType::Handle,
            Self::AtIdentifier(_) => LexiconStringType::AtIdentifier,
            Self::Nsid(_) => LexiconStringType::Nsid,
            Self::AtUri(_) => LexiconStringType::AtUri,
            Self::Uri(uri) => LexiconStringType::Uri(match uri {
                UriValue::Did(_) => UriType::Did,
                UriValue::At(_) => UriType::At,
                UriValue::Https(_) => UriType::Https,
                UriValue::Wss(_) => UriType::Wss,
                UriValue::Cid(_) => UriType::Cid,
                UriValue::Any(_) => UriType::Any,
            }),
            Self::Cid(_) => LexiconStringType::Cid,
            Self::Tid(_) => LexiconStringType::Tid,
            Self::Did(_) => LexiconStringType::Did,
            Self::RecordKey(_) => LexiconStringType::RecordKey,
            Self::String(_) => LexiconStringType::String,
        }
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for AtprotoStr<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str> + Serialize> Serialize for AtprotoStr<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, S> Deserialize<'de> for AtprotoStr<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = S::deserialize(deserializer)?;
        Ok(Self::new(value))
    }
}

impl<S: Bos<str> + AsRef<str>> AtprotoStr<S> {
    /// Convert to an `AtprotoStr` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> AtprotoStr<B> {
        match self {
            AtprotoStr::Datetime(dt) => AtprotoStr::Datetime(dt),
            AtprotoStr::Language(lang) => AtprotoStr::Language(lang),
            AtprotoStr::Tid(tid) => AtprotoStr::Tid(tid),
            AtprotoStr::Nsid(nsid) => AtprotoStr::Nsid(nsid.convert()),
            AtprotoStr::Did(did) => AtprotoStr::Did(did.convert()),
            AtprotoStr::Handle(handle) => AtprotoStr::Handle(handle.convert()),
            AtprotoStr::AtIdentifier(ident) => AtprotoStr::AtIdentifier(ident.convert()),
            AtprotoStr::AtUri(at_uri) => AtprotoStr::AtUri(at_uri.convert()),
            AtprotoStr::Uri(uri) => AtprotoStr::Uri(uri.convert()),
            AtprotoStr::Cid(cid) => AtprotoStr::Cid(cid.convert()),
            AtprotoStr::RecordKey(rkey) => AtprotoStr::RecordKey(RecordKey(rkey.0.convert())),
            AtprotoStr::String(s) => AtprotoStr::String(B::from(s)),
        }
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for AtprotoStr<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = AtprotoStr<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            AtprotoStr::Datetime(datetime) => AtprotoStr::Datetime(datetime),
            AtprotoStr::Language(language) => AtprotoStr::Language(language),
            AtprotoStr::Tid(tid) => AtprotoStr::Tid(tid),
            AtprotoStr::Nsid(nsid) => AtprotoStr::Nsid(nsid.into_static()),
            AtprotoStr::Did(did) => AtprotoStr::Did(did.into_static()),
            AtprotoStr::Handle(handle) => AtprotoStr::Handle(handle.into_static()),
            AtprotoStr::AtIdentifier(ident) => AtprotoStr::AtIdentifier(ident.into_static()),
            // AtUri and UriValue are already 'static in this enum.
            AtprotoStr::AtUri(at_uri) => AtprotoStr::AtUri(at_uri.into_static()),
            AtprotoStr::Uri(uri) => AtprotoStr::Uri(uri.into_static()),
            AtprotoStr::Cid(cid) => AtprotoStr::Cid(cid.into_static()),
            AtprotoStr::RecordKey(record_key) => AtprotoStr::RecordKey(record_key.into_static()),
            AtprotoStr::String(s) => AtprotoStr::String(s.into_static()),
        }
    }
}

impl<S: Bos<str> + AsRef<str> + Clone + Serialize> From<AtprotoStr<S>> for String {
    fn from(value: AtprotoStr<S>) -> Self {
        value.as_str().to_string()
    }
}

/// Parsing Error for atproto string types which don't have third-party specs
/// (e.g. datetime, CIDs, language tags).
///
/// `spec` refers to the final url path segment on atproto.com/specs,
/// detailing the specification for the type
/// `source` is the source string, or part of it
/// `kind` is the type of parsing error: `[StrParseKind]`
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
#[error("error in `{source}`: {kind}")]
#[cfg_attr(
    feature = "std",
    diagnostic(
        url("https://atproto.com/specs/{spec}"),
        help("if something doesn't match the spec, contact the crate author")
    )
)]
pub struct AtStrError {
    /// AT Protocol spec name this error relates to
    pub spec: SmolStr,
    /// The source string that failed to parse
    #[cfg_attr(feature = "std", source_code)]
    pub source: String,
    /// The specific kind of parsing error
    #[source]
    #[cfg_attr(feature = "std", diagnostic_source)]
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
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
pub enum StrParseKind {
    /// Regex pattern validation failed
    #[error("regex failure - {message}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::types::string::regex_fail)))]
    RegexFail {
        /// Optional span highlighting the problem area
        #[cfg_attr(feature = "std", label)]
        span: Option<SourceSpan>,
        /// Help message explaining the failure
        #[cfg_attr(feature = "std", help)]
        message: SmolStr,
    },
    /// String exceeds maximum allowed length
    #[error("string too long (allowed: {max}, actual: {actual})")]
    #[cfg_attr(
        feature = "std",
        diagnostic(code(jacquard::types::string::wrong_length))
    )]
    TooLong {
        /// Maximum allowed length
        max: usize,
        /// Actual string length
        actual: usize,
    },

    /// String is below minimum required length
    #[error("string too short (allowed: {min}, actual: {actual})")]
    #[cfg_attr(
        feature = "std",
        diagnostic(code(jacquard::types::string::wrong_length))
    )]
    TooShort {
        /// Minimum required length
        min: usize,
        /// Actual string length
        actual: usize,
    },
    /// String contains disallowed values
    #[error("disallowed - {message}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::types::string::disallowed)))]
    Disallowed {
        /// Optional span highlighting the disallowed content
        #[cfg_attr(feature = "std", label)]
        problem: Option<SourceSpan>,
        /// Help message about what's disallowed
        #[cfg_attr(feature = "std", help)]
        message: SmolStr,
    },
    /// Required component is missing
    #[error("missing - {message}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::atstr::missing_component)))]
    MissingComponent {
        /// Optional span where the component should be
        #[cfg_attr(feature = "std", label)]
        span: Option<SourceSpan>,
        /// Help message about what's missing
        #[cfg_attr(feature = "std", help)]
        message: SmolStr,
    },
    /// Wraps another error with additional context
    #[error("{err:?}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::atstr::inner)))]
    Wrap {
        /// Optional span in the outer context
        #[cfg_attr(feature = "std", label)]
        span: Option<SourceSpan>,
        /// The wrapped inner error
        #[source]
        err: Arc<AtStrError>,
    },
    /// Wraps another error with additional context
    #[error("converting from a string slice")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::atstr::conversion)))]
    Conversion,
}
