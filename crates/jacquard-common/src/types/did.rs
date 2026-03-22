use crate::bos::{Bos, DefaultStr};
use crate::types::string::{AtStrError, StrParseKind};
use crate::{CowStr, IntoStatic};
use alloc::string::{String, ToString};
use core::fmt;
use core::ops::Deref;
use core::str::FromStr;
#[cfg(all(not(target_arch = "wasm32"), feature = "std"))]
use regex::Regex;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "std")))]
use regex_automata::meta::Regex;
#[cfg(target_arch = "wasm32")]
use regex_lite::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use smol_str::{SmolStr, ToSmolStr};

use super::Lazy;

/// Decentralized Identifier (DID) for AT Protocol accounts.
///
/// DIDs are the persistent, long-term account identifiers in AT Protocol. Unlike handles,
/// which can change, a DID permanently identifies an account across the network.
///
/// See: <https://atproto.com/specs/did>
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Did<S: Bos<str> = DefaultStr>(pub(crate) S);

/// Regex for DID validation per AT Protocol spec.
///
/// Note: This regex allows `%` in the identifier but prevents DIDs from ending with `:` or `%`.
/// It does NOT validate that percent-encoding is well-formed (i.e., `%XX` where XX are hex digits).
/// This matches the behavior of the official TypeScript implementation.
pub static DID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^did:[a-z]+:[a-zA-Z0-9._:%-]*[a-zA-Z0-9._-]$").unwrap());

// ---------------------------------------------------------------------------
// Shared validation
// ---------------------------------------------------------------------------

fn strip_did_prefix(did: &str) -> &str {
    did.strip_prefix("at://").unwrap_or(did)
}

pub(crate) fn validate_did(did: &str) -> Result<(), AtStrError> {
    if did.len() > 2048 {
        Err(AtStrError::too_long("did", did, 2048, did.len()))
    } else if !DID_REGEX.is_match(did) {
        Err(AtStrError::regex(
            "did",
            did,
            SmolStr::new_static("invalid"),
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Core methods
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Did<S> {
    /// Get the DID as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl<S: Bos<str>> Did<S> {
    /// Infallible unchecked constructor.
    ///
    /// # Safety
    ///
    /// The caller must ensure the DID is valid.
    pub unsafe fn unchecked(did: S) -> Self {
        Did(did)
    }
}

// ---------------------------------------------------------------------------
// Generic construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Did<S> {
    /// Fallible constructor, validates, wraps the input directly.
    ///
    /// Does NOT strip `at://` prefix — use `new_owned()` for that.
    pub fn new(s: S) -> Result<Self, AtStrError> {
        validate_did(s.as_ref())?;
        Ok(Did(s))
    }

    /// Infallible constructor. Panics on invalid DIDs.
    pub fn raw(s: S) -> Self {
        Self::new(s).expect("invalid DID")
    }
}

// ---------------------------------------------------------------------------
// Owned construction (with prefix stripping)
// ---------------------------------------------------------------------------

impl<S: Bos<str> + FromStr> Did<S> {
    /// Fallible constructor, validates, takes ownership.
    ///
    /// Accepts (and strips) preceding `at://` if present.
    pub fn new_owned(did: impl AsRef<str>) -> Result<Self, AtStrError> {
        let did = did.as_ref();
        let stripped = strip_did_prefix(did);
        validate_did(stripped)?;
        // FromStr for backing types (SmolStr, String, CowStr) is infallible.
        let s = S::from_str(stripped)
            .map_err(|_| AtStrError::new("did", stripped.to_string(), StrParseKind::Conversion))?;
        Ok(Self(s))
    }

    /// Fallible constructor for static strings.
    pub fn new_static(did: &'static str) -> Result<Self, AtStrError> {
        let stripped = strip_did_prefix(did);
        validate_did(stripped)?;
        let s = S::from_str(stripped)
            .map_err(|_| AtStrError::new("did", stripped.to_string(), StrParseKind::Conversion))?;
        Ok(Self(s))
    }
}

// ---------------------------------------------------------------------------
// Deserialization
// ---------------------------------------------------------------------------

impl<'de, S> Deserialize<'de> for Did<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        validate_did(s.as_ref()).map_err(serde::de::Error::custom)?;
        Ok(Did(s))
    }
}

// ---------------------------------------------------------------------------
// Trait impls
// ---------------------------------------------------------------------------

impl<S: Bos<str> + IntoStatic> IntoStatic for Did<S>
where
    S::Output: Bos<str>,
{
    type Output = Did<S::Output>;

    fn into_static(self) -> Self::Output {
        Did(self.0.into_static())
    }
}

impl FromStr for Did {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for Did<CowStr<'static>> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for Did<String> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for Did<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Debug for Did<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}", self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> From<Did<S>> for String {
    fn from(value: Did<S>) -> Self {
        value.as_str().to_string()
    }
}

impl<S: Bos<str> + AsRef<str>> From<Did<S>> for CowStr<'static> {
    fn from(value: Did<S>) -> Self {
        CowStr::copy_from_str(value.as_str())
    }
}

impl From<String> for Did {
    fn from(value: String) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<'d> From<CowStr<'d>> for Did<CowStr<'d>> {
    fn from(value: CowStr<'d>) -> Self {
        Self::new(value).unwrap()
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for Did<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for Did<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_dids() {
        assert!(Did::<&str>::new("did:plc:abc123").is_ok());
        assert!(Did::<&str>::new("did:web:example.com").is_ok());
        assert!(Did::<&str>::new("did:method:val_ue").is_ok());
        assert!(Did::<&str>::new("did:method:val-ue").is_ok());
        assert!(Did::<&str>::new("did:method:val.ue").is_ok());
        assert!(Did::<&str>::new("did:method:val%20ue").is_ok());
    }

    #[test]
    fn valid_dids_owned() {
        assert!(Did::<SmolStr>::new_owned("did:plc:abc123").is_ok());
        assert!(Did::<String>::new_owned("did:web:example.com").is_ok());
    }

    #[test]
    fn prefix_stripping() {
        // new() does not strip — use new_owned() for that.
        assert!(Did::<&str>::new("at://did:plc:foo").is_err());
        assert_eq!(
            Did::<SmolStr>::new_owned("at://did:plc:foo").unwrap().as_str(),
            "did:plc:foo"
        );
        assert_eq!(
            Did::<&str>::new("did:plc:foo").unwrap().as_str(),
            "did:plc:foo"
        );
    }

    #[test]
    fn must_start_with_did() {
        assert!(Did::<&str>::new("DID:plc:foo").is_err());
        assert!(Did::<&str>::new("plc:foo").is_err());
        assert!(Did::<&str>::new("foo").is_err());
    }

    #[test]
    fn method_must_be_lowercase() {
        assert!(Did::<&str>::new("did:plc:foo").is_ok());
        assert!(Did::<&str>::new("did:PLC:foo").is_err());
        assert!(Did::<&str>::new("did:Plc:foo").is_err());
    }

    #[test]
    fn cannot_end_with_colon_or_percent() {
        assert!(Did::<&str>::new("did:plc:foo:").is_err());
        assert!(Did::<&str>::new("did:plc:foo%").is_err());
        assert!(Did::<&str>::new("did:plc:foo:bar").is_ok());
    }

    #[test]
    fn max_length() {
        let valid_2048 = format!("did:plc:{}", "a".repeat(2048 - 8));
        assert_eq!(valid_2048.len(), 2048);
        assert!(Did::<&str>::new(&valid_2048).is_ok());

        let too_long_2049 = format!("did:plc:{}", "a".repeat(2049 - 8));
        assert_eq!(too_long_2049.len(), 2049);
        assert!(Did::<&str>::new(&too_long_2049).is_err());
    }

    #[test]
    fn allowed_characters() {
        assert!(Did::<&str>::new("did:method:abc123").is_ok());
        assert!(Did::<&str>::new("did:method:ABC123").is_ok());
        assert!(Did::<&str>::new("did:method:a_b_c").is_ok());
        assert!(Did::<&str>::new("did:method:a-b-c").is_ok());
        assert!(Did::<&str>::new("did:method:a.b.c").is_ok());
        assert!(Did::<&str>::new("did:method:a:b:c").is_ok());
    }

    #[test]
    fn disallowed_characters() {
        assert!(Did::<&str>::new("did:method:a b").is_err());
        assert!(Did::<&str>::new("did:method:a@b").is_err());
        assert!(Did::<&str>::new("did:method:a#b").is_err());
        assert!(Did::<&str>::new("did:method:a?b").is_err());
    }

    #[test]
    fn percent_encoding() {
        assert!(Did::<&str>::new("did:method:foo%20bar").is_ok());
        assert!(Did::<&str>::new("did:method:foo%2Fbar").is_ok());
        assert!(Did::<&str>::new("did:method:foo%").is_err());
        // Matches TS reference impl: malformed percent-encoding accepted.
        assert!(Did::<&str>::new("did:method:foo%2x").is_ok());
        assert!(Did::<&str>::new("did:method:foo%ZZ").is_ok());
    }

    #[test]
    fn into_static() {
        let d = Did::<&str>::new("did:plc:abc123").unwrap();
        let owned: Did<SmolStr> = d.into_static();
        assert_eq!(owned.as_str(), "did:plc:abc123");
    }

    #[test]
    fn cross_type_equality() {
        let borrowed = Did::<&str>::new("did:plc:abc123").unwrap();
        let owned = Did::<SmolStr>::new_owned("did:plc:abc123").unwrap();
        // Different S types but same content — PartialEq compares the inner S values.
        assert_eq!(borrowed.as_str(), owned.as_str());
    }
}
