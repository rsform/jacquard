use crate::bos::{Bos, DefaultStr};
use crate::types::string::{AtStrError, StrParseKind};
use crate::types::{DISALLOWED_TLDS, ends_with};
use crate::{CowStr, IntoStatic};
use alloc::string::String;
use alloc::string::ToString;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use core::str::FromStr;
#[cfg(all(not(target_arch = "wasm32"), feature = "std"))]
use regex::Regex;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "std")))]
use regex_automata::meta::Regex;
#[cfg(target_arch = "wasm32")]
use regex_lite::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::{SmolStr, StrExt};

use super::Lazy;

/// AT Protocol handle (human-readable account identifier).
///
/// # Case semantics
///
/// Handle is **case-preserving but case-insensitive**: the stored string retains its
/// original casing, but equality, hashing, serialization, and display all operate on
/// the lowercased form. `as_str()` returns the raw stored value.
///
/// See: <https://atproto.com/specs/handle>
#[derive(Clone)]
#[repr(transparent)]
pub struct Handle<S: Bos<str> = DefaultStr>(pub(crate) S);

/// Regex for handle validation per AT Protocol spec.
pub static HANDLE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$").unwrap()
});

// ---------------------------------------------------------------------------
// Shared validation
// ---------------------------------------------------------------------------

fn strip_handle_prefix(handle: &str) -> &str {
    handle
        .strip_prefix("at://")
        .or_else(|| handle.strip_prefix('@'))
        .unwrap_or(handle)
}

pub(crate) fn validate_handle(handle: &str) -> Result<(), AtStrError> {
    if handle.len() > 253 {
        Err(AtStrError::too_long("handle", handle, 253, handle.len()))
    } else if !HANDLE_REGEX.is_match(handle) {
        Err(AtStrError::regex(
            "handle",
            handle,
            SmolStr::new_static("invalid"),
        ))
    } else if ends_with(handle, DISALLOWED_TLDS) && handle != "handle.invalid" {
        Err(AtStrError::disallowed("handle", handle, DISALLOWED_TLDS))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Core methods
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Handle<S> {
    /// Get the handle as a string slice.
    ///
    /// Returns the raw stored value, which may contain uppercase if the handle
    /// was deserialized from non-canonical wire data. For canonical output,
    /// use `Display` or `Serialize`.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Confirm that this is a (syntactically) valid handle (as we pass-through
    /// "handle.invalid" during construction).
    pub fn is_valid(&self) -> bool {
        let s = self.as_str();
        s.len() <= 253 && HANDLE_REGEX.is_match(s) && !ends_with(s, DISALLOWED_TLDS)
    }
}

impl<S: Bos<str>> Handle<S> {
    /// Infallible unchecked constructor.
    ///
    /// # Safety
    ///
    /// The caller must ensure the handle is valid.
    pub unsafe fn unchecked(handle: S) -> Self {
        Handle(handle)
    }
}

// ---------------------------------------------------------------------------
// Borrowed construction: Handle<&'h str>
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Handle<S> {
    /// Fallible constructor, validates, wraps the input directly.
    ///
    /// Rejects uppercase input — use `new_owned()` for case-insensitive construction.
    /// Does NOT strip `@` or `at://` prefix — use `new_owned()` for that.
    pub fn new(s: S) -> Result<Self, AtStrError> {
        let r = s.as_ref();
        if r.contains(|c: char| c.is_ascii_uppercase()) {
            return Err(AtStrError::regex(
                "handle",
                r,
                SmolStr::new_static("contains uppercase (use new_owned for normalisation)"),
            ));
        }
        validate_handle(r)?;
        Ok(Self(s))
    }

    /// Infallible constructor. Panics on invalid handles.
    pub fn raw(s: S) -> Self {
        Self::new(s).expect("invalid handle")
    }
}

// ---------------------------------------------------------------------------
// Owned construction (with prefix stripping and normalisation)
// ---------------------------------------------------------------------------

impl<S: Bos<str> + FromStr> Handle<S> {
    /// Fallible constructor, validates, takes ownership. Normalises to lowercase.
    ///
    /// Accepts (and strips) preceding `@` or `at://` if present.
    pub fn new_owned(handle: impl AsRef<str>) -> Result<Self, AtStrError> {
        let handle = handle.as_ref();
        let stripped = strip_handle_prefix(handle);
        let normalized = stripped.to_lowercase_smolstr();
        validate_handle(&normalized)?;
        let s = S::from_str(&normalized).map_err(|_| {
            AtStrError::new("handle", normalized.to_string(), StrParseKind::Conversion)
        })?;
        Ok(Self(s))
    }

    /// Fallible constructor for static strings. Normalises to lowercase.
    pub fn new_static(handle: &'static str) -> Result<Self, AtStrError> {
        let stripped = strip_handle_prefix(handle);
        let normalized = if stripped.contains(|c: char| c.is_ascii_uppercase()) {
            stripped.to_lowercase_smolstr()
        } else {
            SmolStr::new_static(stripped)
        };
        validate_handle(&normalized)?;
        let s = S::from_str(&normalized).map_err(|_| {
            AtStrError::new("handle", normalized.to_string(), StrParseKind::Conversion)
        })?;
        Ok(Self(s))
    }
}

// ---------------------------------------------------------------------------
// Deserialization — generic over S: Deserialize<'de>
// ---------------------------------------------------------------------------

impl<'de, S> Deserialize<'de> for Handle<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        validate_handle(s.as_ref()).map_err(serde::de::Error::custom)?;
        Ok(Handle(s))
    }
}

// ---------------------------------------------------------------------------
// Serialization — always lowercase
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Serialize for Handle<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        let raw = self.as_str();
        if raw.bytes().all(|b| !b.is_ascii_uppercase()) {
            serializer.serialize_str(raw)
        } else {
            let lowered = raw.to_lowercase_smolstr();
            serializer.serialize_str(&lowered)
        }
    }
}

// ---------------------------------------------------------------------------
// Case-insensitive equality and hashing
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>, T: Bos<str> + AsRef<str>> PartialEq<Handle<T>> for Handle<S> {
    fn eq(&self, other: &Handle<T>) -> bool {
        self.as_str().eq_ignore_ascii_case(other.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> Eq for Handle<S> {}

impl<S: Bos<str> + AsRef<str>> Hash for Handle<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for byte in self.as_str().bytes() {
            state.write_u8(byte.to_ascii_lowercase());
        }
    }
}

// ---------------------------------------------------------------------------
// Other trait impls
// ---------------------------------------------------------------------------

impl<S: Bos<str> + IntoStatic> IntoStatic for Handle<S>
where
    S::Output: Bos<str>,
{
    type Output = Handle<S::Output>;

    fn into_static(self) -> Self::Output {
        Handle(self.0.into_static())
    }
}

impl FromStr for Handle {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for Handle<CowStr<'static>> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for Handle<String> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for Handle<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.as_ref().contains(|c: char| c.is_ascii_uppercase()) {
            for c in self.as_str().chars() {
                fmt::Write::write_char(f, c.to_ascii_lowercase())?;
            }
        } else {
            f.write_str(self.as_str())?;
        }
        Ok(())
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Debug for Handle<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://")?;
        if self.0.as_ref().contains(|c: char| c.is_ascii_uppercase()) {
            for c in self.as_str().chars() {
                fmt::Write::write_char(f, c.to_ascii_lowercase())?;
            }
        } else {
            f.write_str(self.as_str())?;
        }
        Ok(())
    }
}

impl<S: Bos<str> + AsRef<str>> From<Handle<S>> for String {
    fn from(value: Handle<S>) -> Self {
        value.as_str().to_ascii_lowercase()
    }
}

impl<S: Bos<str> + AsRef<str>> From<Handle<S>> for SmolStr {
    fn from(value: Handle<S>) -> Self {
        value.as_str().to_ascii_lowercase_smolstr()
    }
}

impl From<String> for Handle {
    fn from(value: String) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<'h> From<CowStr<'h>> for Handle<CowStr<'h>> {
    fn from(value: CowStr<'h>) -> Self {
        Self::new(value).unwrap()
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for Handle<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for Handle<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_handles() {
        assert!(Handle::<&str>::new("alice.test").is_ok());
        assert!(Handle::<&str>::new("foo.bsky.social").is_ok());
        assert!(Handle::<&str>::new("a.b.c.d.e").is_ok());
        assert!(Handle::<&str>::new("a1.b2.c3").is_ok());
        assert!(Handle::<&str>::new("name-with-dash.com").is_ok());
    }

    #[test]
    fn valid_handles_owned() {
        assert!(Handle::<SmolStr>::new_owned("alice.test").is_ok());
        assert!(Handle::<SmolStr>::new_owned("Alice.Test").is_ok());
        assert!(Handle::<String>::new_owned("foo.bsky.social").is_ok());
    }

    #[test]
    fn borrowed_rejects_uppercase() {
        assert!(Handle::<&str>::new("Alice.Test").is_err());
    }

    #[test]
    fn prefix_stripping() {
        // new() does not strip — use new_owned() for that.
        assert!(Handle::<&str>::new("@alice.test").is_err());
        assert!(Handle::<&str>::new("at://alice.test").is_err());
        assert_eq!(
            Handle::<SmolStr>::new_owned("@alice.test").unwrap().as_str(),
            "alice.test"
        );
        assert_eq!(
            Handle::<SmolStr>::new_owned("at://alice.test").unwrap().as_str(),
            "alice.test"
        );
        assert_eq!(
            Handle::<&str>::new("alice.test").unwrap().as_str(),
            "alice.test"
        );
    }

    #[test]
    fn prefix_stripping_owned() {
        assert_eq!(
            Handle::<SmolStr>::new_owned("@Alice.Test")
                .unwrap()
                .as_str(),
            "alice.test"
        );
        assert_eq!(
            Handle::<SmolStr>::new_owned("at://alice.test")
                .unwrap()
                .as_str(),
            "alice.test"
        );
    }

    #[test]
    fn max_length() {
        let s1 = format!("a{}a", "b".repeat(61));
        let s2 = format!("c{}c", "d".repeat(61));
        let s3 = format!("e{}e", "f".repeat(61));
        let s4 = format!("g{}g", "h".repeat(59));
        let valid_253 = format!("{s1}.{s2}.{s3}.{s4}");
        assert_eq!(valid_253.len(), 253);
        assert!(Handle::<&str>::new(&valid_253).is_ok());

        let s4_long = format!("g{}g", "h".repeat(60));
        let too_long_254 = format!("{s1}.{s2}.{s3}.{s4_long}");
        assert_eq!(too_long_254.len(), 254);
        assert!(Handle::<&str>::new(&too_long_254).is_err());
    }

    #[test]
    fn segment_length_constraints() {
        let valid = format!("{}.com", "a".repeat(63));
        assert!(Handle::<&str>::new(&valid).is_ok());
        let too_long = format!("{}.com", "a".repeat(64));
        assert!(Handle::<&str>::new(&too_long).is_err());
    }

    #[test]
    fn hyphen_placement() {
        assert!(Handle::<&str>::new("valid-label.com").is_ok());
        assert!(Handle::<&str>::new("-nope.com").is_err());
        assert!(Handle::<&str>::new("nope-.com").is_err());
    }

    #[test]
    fn tld_must_start_with_letter() {
        assert!(Handle::<&str>::new("foo.bar").is_ok());
        assert!(Handle::<&str>::new("foo.9bar").is_err());
    }

    #[test]
    fn disallowed_tlds() {
        for tld in [
            "local",
            "localhost",
            "arpa",
            "invalid",
            "internal",
            "example",
            "alt",
            "onion",
        ] {
            assert!(
                Handle::<&str>::new(&format!("foo.{tld}")).is_err(),
                "should reject .{tld}"
            );
        }
    }

    #[test]
    fn minimum_segments() {
        assert!(Handle::<&str>::new("a.b").is_ok());
        assert!(Handle::<&str>::new("a").is_err());
        assert!(Handle::<&str>::new("com").is_err());
    }

    #[test]
    fn invalid_characters() {
        assert!(Handle::<&str>::new("foo!bar.com").is_err());
        assert!(Handle::<&str>::new("foo_bar.com").is_err());
        assert!(Handle::<&str>::new("foo bar.com").is_err());
        assert!(Handle::<&str>::new("foo@bar.com").is_err());
    }

    #[test]
    fn empty_segments() {
        assert!(Handle::<&str>::new("foo..com").is_err());
        assert!(Handle::<&str>::new(".foo.com").is_err());
        assert!(Handle::<&str>::new("foo.com.").is_err());
    }

    #[test]
    fn handle_invalid_passthrough() {
        assert!(Handle::<&str>::new("handle.invalid").is_ok());
        assert!(Handle::<SmolStr>::new_owned("handle.invalid").is_ok());
    }

    #[test]
    fn into_static_borrowed() {
        let h = Handle::<&str>::new("alice.test").unwrap();
        let owned: Handle<SmolStr> = h.into_static();
        assert_eq!(owned.as_str(), "alice.test");
    }

    #[test]
    fn into_static_already_owned() {
        let h = Handle::<SmolStr>::new_owned("alice.test").unwrap();
        let owned: Handle<SmolStr> = h.into_static();
        assert_eq!(owned.as_str(), "alice.test");
    }

    #[test]
    fn case_insensitive_equality() {
        let lower = Handle::<SmolStr>::new_owned("alice.test").unwrap();
        let upper = Handle(SmolStr::new("Alice.Test"));
        assert_eq!(lower, upper);
    }

    #[test]
    fn case_insensitive_hash() {
        let a = Handle::<SmolStr>::new_owned("alice.test").unwrap();
        let b = Handle(SmolStr::new("Alice.Test"));
        assert_eq!(a, b);
        #[allow(deprecated)]
        let (mut ha, mut hb) = (core::hash::SipHasher::new(), core::hash::SipHasher::new());
        a.hash(&mut ha);
        b.hash(&mut hb);
        assert_eq!(ha.finish(), hb.finish());
    }

    #[test]
    fn display_lowercases() {
        let h = Handle(SmolStr::new("Alice.Test"));
        assert_eq!(format!("{h}"), "alice.test");
    }

    #[test]
    fn serialize_lowercases() {
        let h = Handle(SmolStr::new("Alice.Test"));
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json, "\"alice.test\"");
    }

    #[test]
    fn cross_type_equality() {
        let borrowed = Handle::<&str>::new("alice.test").unwrap();
        let owned = Handle::<SmolStr>::new_owned("alice.test").unwrap();
        assert_eq!(borrowed, owned);
    }
}
