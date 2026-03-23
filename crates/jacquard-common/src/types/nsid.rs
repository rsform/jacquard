use crate::bos::{Bos, DefaultStr};
use crate::types::recordkey::RecordKeyType;
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

/// Namespaced Identifier (NSID) for Lexicon schemas and XRPC endpoints.
///
/// See: <https://atproto.com/specs/nsid>
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Nsid<S: Bos<str> = DefaultStr>(pub(crate) S);

/// Regex for NSID validation per AT Protocol spec.
pub static NSID_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+(\.[a-zA-Z][a-zA-Z0-9]{0,62})$").unwrap()
});

pub(crate) fn validate_nsid(nsid: &str) -> Result<(), AtStrError> {
    if nsid.len() > 317 {
        Err(AtStrError::too_long("nsid", nsid, 317, nsid.len()))
    } else if !NSID_REGEX.is_match(nsid) {
        Err(AtStrError::regex(
            "nsid",
            nsid,
            SmolStr::new_static("invalid"),
        ))
    } else {
        Ok(())
    }
}

impl<S: Bos<str> + AsRef<str>> Nsid<S> {
    /// Get the NSID as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Returns the domain authority part of the NSID.
    pub fn domain_authority(&self) -> &str {
        let s = self.as_str();
        let split = s.rfind('.').expect("enforced by constructor");
        &s[..split]
    }

    /// Returns the name segment of the NSID.
    pub fn name(&self) -> &str {
        let s = self.as_str();
        let split = s.rfind('.').expect("enforced by constructor");
        &s[split + 1..]
    }
}

impl<S: Bos<str>> Nsid<S> {
    /// # Safety
    ///
    /// The caller must ensure the NSID is valid.
    pub unsafe fn unchecked(nsid: S) -> Self {
        Nsid(nsid)
    }

    /// Borrow as an `Nsid<&str>`, analogous to `Uri::borrow()`.
    pub fn borrow(&self) -> Nsid<&str>
    where
        S: AsRef<str>,
    {
        // SAFETY: self is already validated.
        unsafe { Nsid::unchecked(self.0.as_ref()) }
    }
}

impl<S: Bos<str> + AsRef<str>> Nsid<S> {
    /// Fallible constructor, validates, wraps the input directly.
    pub fn new(s: S) -> Result<Self, AtStrError> {
        validate_nsid(s.as_ref())?;
        Ok(Self(s))
    }

    /// Infallible constructor. Panics on invalid NSIDs.
    pub fn raw(s: S) -> Self {
        Self::new(s).expect("invalid NSID")
    }
}

impl<S: Bos<str> + FromStr> Nsid<S> {
    /// Fallible constructor, validates, takes ownership.
    pub fn new_owned(nsid: impl AsRef<str>) -> Result<Self, AtStrError> {
        let nsid = nsid.as_ref();
        validate_nsid(nsid)?;
        let s = S::from_str(nsid)
            .map_err(|_| AtStrError::new("nsid", nsid.to_string(), StrParseKind::Conversion))?;
        Ok(Self(s))
    }

    /// Fallible constructor for static strings.
    pub fn new_static(nsid: &'static str) -> Result<Self, AtStrError> {
        validate_nsid(nsid)?;
        let s = S::from_str(nsid)
            .map_err(|_| AtStrError::new("nsid", nsid.to_string(), StrParseKind::Conversion))?;
        Ok(Self(s))
    }
}

impl<'de, S> Deserialize<'de> for Nsid<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        validate_nsid(s.as_ref()).map_err(serde::de::Error::custom)?;
        Ok(Nsid(s))
    }
}

impl<S: Bos<str> + IntoStatic> IntoStatic for Nsid<S>
where
    S::Output: Bos<str>,
{
    type Output = Nsid<S::Output>;

    fn into_static(self) -> Self::Output {
        Nsid(self.0.into_static())
    }
}

impl<S: Bos<str>> Nsid<S> {
    /// Convert to an `Nsid` with a different backing type.
    pub fn convert<B: Bos<str> + From<S>>(self) -> Nsid<B> {
        Nsid(B::from(self.0))
    }
}

impl<S: Bos<str> + FromStr> FromStr for Nsid<S> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for Nsid<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Debug for Nsid<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}", self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> From<Nsid<S>> for String {
    fn from(value: Nsid<S>) -> Self {
        value.as_str().to_string()
    }
}

impl<S: Bos<str> + AsRef<str>> From<Nsid<S>> for SmolStr {
    fn from(value: Nsid<S>) -> Self {
        value.as_str().to_smolstr()
    }
}

impl From<String> for Nsid {
    fn from(value: String) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<'n> From<CowStr<'n>> for Nsid<CowStr<'n>> {
    fn from(value: CowStr<'n>) -> Self {
        Self::new(value).unwrap()
    }
}

impl From<SmolStr> for Nsid {
    fn from(value: SmolStr) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for Nsid<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for Nsid<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

unsafe impl<S: Bos<str> + AsRef<str> + Clone + Serialize> RecordKeyType for Nsid<S> {
    fn as_str(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_nsids() {
        assert!(Nsid::<&str>::new("com.example.foo").is_ok());
        assert!(Nsid::<&str>::new("com.example.fooBar").is_ok());
        assert!(Nsid::<&str>::new("com.long-domain.foo").is_ok());
        assert!(Nsid::<&str>::new("a.b.c").is_ok());
        assert!(Nsid::<&str>::new("a1.b2.c3").is_ok());
    }

    #[test]
    fn minimum_segments() {
        assert!(Nsid::<&str>::new("a.b.c").is_ok());
        assert!(Nsid::<&str>::new("a.b").is_err());
        assert!(Nsid::<&str>::new("a").is_err());
    }

    #[test]
    fn domain_and_name_parsing() {
        let nsid = Nsid::<&str>::new("com.example.fooBar").unwrap();
        assert_eq!(nsid.domain_authority(), "com.example");
        assert_eq!(nsid.name(), "fooBar");
    }

    #[test]
    fn max_length() {
        let s1 = format!("a{}a", "b".repeat(61));
        let s2 = format!("c{}c", "d".repeat(61));
        let s3 = format!("e{}e", "f".repeat(61));
        let s4 = format!("g{}g", "h".repeat(61));
        let s5 = format!("i{}i", "j".repeat(59));
        let valid_317 = format!("{s1}.{s2}.{s3}.{s4}.{s5}");
        assert_eq!(valid_317.len(), 317);
        assert!(Nsid::<&str>::new(&valid_317).is_ok());

        let s5_long = format!("i{}i", "j".repeat(60));
        let too_long_318 = format!("{s1}.{s2}.{s3}.{s4}.{s5_long}");
        assert_eq!(too_long_318.len(), 318);
        assert!(Nsid::<&str>::new(&too_long_318).is_err());
    }

    #[test]
    fn segment_length() {
        let valid_63 = format!("{}.{}.foo", "a".repeat(63), "b".repeat(63));
        assert!(Nsid::<&str>::new(&valid_63).is_ok());

        let too_long_64 = format!("{}.b.foo", "a".repeat(64));
        assert!(Nsid::<&str>::new(&too_long_64).is_err());
    }

    #[test]
    fn first_segment_cannot_start_with_digit() {
        assert!(Nsid::<&str>::new("com.example.foo").is_ok());
        assert!(Nsid::<&str>::new("9com.example.foo").is_err());
    }

    #[test]
    fn name_segment_rules() {
        assert!(Nsid::<&str>::new("com.example.foo").is_ok());
        assert!(Nsid::<&str>::new("com.example.fooBar123").is_ok());
        assert!(Nsid::<&str>::new("com.example.9foo").is_err());
        assert!(Nsid::<&str>::new("com.example.foo-bar").is_err());
    }

    #[test]
    fn domain_segment_rules() {
        assert!(Nsid::<&str>::new("foo-bar.example.baz").is_ok());
        assert!(Nsid::<&str>::new("foo.bar-baz.qux").is_ok());
        assert!(Nsid::<&str>::new("-foo.bar.baz").is_err());
        assert!(Nsid::<&str>::new("foo-.bar.baz").is_err());
    }

    #[test]
    fn case_sensitivity() {
        assert!(Nsid::<&str>::new("com.example.fooBar").is_ok());
        assert!(Nsid::<&str>::new("com.example.FooBar").is_ok());
    }

    #[test]
    fn into_static() {
        let n = Nsid::<&str>::new("com.example.foo").unwrap();
        let owned: Nsid<SmolStr> = n.into_static();
        assert_eq!(owned.as_str(), "com.example.foo");
    }
}
