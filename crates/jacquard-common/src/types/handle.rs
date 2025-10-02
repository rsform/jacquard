use crate::types::string::AtStrError;
use crate::types::{DISALLOWED_TLDS, ends_with};
use crate::{CowStr, IntoStatic};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

#[derive(Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Handle<'h>(CowStr<'h>);

pub static HANDLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$").unwrap()
});

/// AT Protocol handle
impl<'h> Handle<'h> {
    /// Fallible constructor, validates, borrows from input
    ///
    /// Accepts (and strips) preceding '@' or 'at://' if present
    pub fn new(handle: &'h str) -> Result<Self, AtStrError> {
        let stripped = handle
            .strip_prefix("at://")
            .or_else(|| handle.strip_prefix('@'))
            .unwrap_or(handle);

        if stripped.len() > 253 {
            Err(AtStrError::too_long(
                "handle",
                stripped,
                253,
                stripped.len(),
            ))
        } else if !HANDLE_REGEX.is_match(stripped) {
            Err(AtStrError::regex(
                "handle",
                stripped,
                SmolStr::new_static("invalid"),
            ))
        } else if ends_with(stripped, DISALLOWED_TLDS) {
            Err(AtStrError::disallowed("handle", stripped, DISALLOWED_TLDS))
        } else {
            Ok(Self(CowStr::Borrowed(stripped)))
        }
    }

    /// Fallible constructor, validates, takes ownership
    pub fn new_owned(handle: impl AsRef<str>) -> Result<Self, AtStrError> {
        let handle = handle.as_ref();
        let stripped = handle
            .strip_prefix("at://")
            .or_else(|| handle.strip_prefix('@'))
            .unwrap_or(handle);
        let handle = stripped;
        if handle.len() > 253 {
            Err(AtStrError::too_long("handle", handle, 253, handle.len()))
        } else if !HANDLE_REGEX.is_match(handle) {
            Err(AtStrError::regex(
                "handle",
                handle,
                SmolStr::new_static("invalid"),
            ))
        } else if ends_with(handle, DISALLOWED_TLDS) {
            Err(AtStrError::disallowed("handle", handle, DISALLOWED_TLDS))
        } else {
            Ok(Self(CowStr::Owned(handle.to_smolstr())))
        }
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(handle: &'static str) -> Result<Self, AtStrError> {
        let stripped = handle
            .strip_prefix("at://")
            .or_else(|| handle.strip_prefix('@'))
            .unwrap_or(handle);
        let handle = stripped;
        if handle.len() > 253 {
            Err(AtStrError::too_long("handle", handle, 253, handle.len()))
        } else if !HANDLE_REGEX.is_match(handle) {
            Err(AtStrError::regex(
                "handle",
                handle,
                SmolStr::new_static("invalid"),
            ))
        } else if ends_with(handle, DISALLOWED_TLDS) {
            Err(AtStrError::disallowed("handle", handle, DISALLOWED_TLDS))
        } else {
            Ok(Self(CowStr::new_static(handle)))
        }
    }
    /// Infallible constructor for when you *know* the string is a valid handle.
    /// Will panic on invalid handles. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    ///
    /// Accepts (and strips) preceding '@' or 'at://' if present
    pub fn raw(handle: &'h str) -> Self {
        let stripped = handle
            .strip_prefix("at://")
            .or_else(|| handle.strip_prefix('@'))
            .unwrap_or(handle);
        let handle = stripped;
        if handle.len() > 253 {
            panic!("handle too long")
        } else if !HANDLE_REGEX.is_match(handle) {
            panic!("Invalid handle")
        } else if ends_with(handle, DISALLOWED_TLDS) {
            panic!("top-level domain not allowed in handles")
        } else {
            Self(CowStr::Borrowed(handle))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid handle.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    ///
    /// Accepts (and strips) preceding '@' or 'at://' if present
    pub unsafe fn unchecked(handle: &'h str) -> Self {
        let stripped = handle
            .strip_prefix("at://")
            .or_else(|| handle.strip_prefix('@'))
            .unwrap_or(handle);
        Self(CowStr::Borrowed(stripped))
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Handle<'_> {
    type Err = AtStrError;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `Handle::new()` or `Handle::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl IntoStatic for Handle<'_> {
    type Output = Handle<'static>;

    fn into_static(self) -> Self::Output {
        Handle(self.0.into_static())
    }
}

impl<'de> Deserialize<'de> for Handle<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Handle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Handle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}", self.0)
    }
}

impl<'h> From<Handle<'h>> for String {
    fn from(value: Handle<'h>) -> Self {
        value.0.to_string()
    }
}

impl<'h> From<Handle<'h>> for CowStr<'h> {
    fn from(value: Handle<'h>) -> Self {
        value.0
    }
}

impl From<String> for Handle<'static> {
    fn from(value: String) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<'h> From<CowStr<'h>> for Handle<'h> {
    fn from(value: CowStr<'h>) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl AsRef<str> for Handle<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Handle<'_> {
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
        assert!(Handle::new("alice.test").is_ok());
        assert!(Handle::new("foo.bsky.social").is_ok());
        assert!(Handle::new("a.b.c.d.e").is_ok());
        assert!(Handle::new("a1.b2.c3").is_ok());
        assert!(Handle::new("name-with-dash.com").is_ok());
    }

    #[test]
    fn prefix_stripping() {
        assert_eq!(Handle::new("@alice.test").unwrap().as_str(), "alice.test");
        assert_eq!(
            Handle::new("at://alice.test").unwrap().as_str(),
            "alice.test"
        );
        assert_eq!(Handle::new("alice.test").unwrap().as_str(), "alice.test");
    }

    #[test]
    fn max_length() {
        // 253 chars: three 63-char segments + one 61-char segment + 3 dots = 253
        let s1 = format!("a{}a", "b".repeat(61)); // 63
        let s2 = format!("c{}c", "d".repeat(61)); // 63
        let s3 = format!("e{}e", "f".repeat(61)); // 63
        let s4 = format!("g{}g", "h".repeat(59)); // 61
        let valid_253 = format!("{}.{}.{}.{}", s1, s2, s3, s4);
        assert_eq!(valid_253.len(), 253);
        assert!(Handle::new(&valid_253).is_ok());

        // 254 chars: make last segment 62 chars
        let s4_long = format!("g{}g", "h".repeat(60)); // 62
        let too_long_254 = format!("{}.{}.{}.{}", s1, s2, s3, s4_long);
        assert_eq!(too_long_254.len(), 254);
        assert!(Handle::new(&too_long_254).is_err());
    }

    #[test]
    fn segment_length_constraints() {
        let valid_63_char_segment = format!("{}.com", "a".repeat(63));
        assert!(Handle::new(&valid_63_char_segment).is_ok());

        let too_long_64_char_segment = format!("{}.com", "a".repeat(64));
        assert!(Handle::new(&too_long_64_char_segment).is_err());
    }

    #[test]
    fn hyphen_placement() {
        assert!(Handle::new("valid-label.com").is_ok());
        assert!(Handle::new("-nope.com").is_err());
        assert!(Handle::new("nope-.com").is_err());
    }

    #[test]
    fn tld_must_start_with_letter() {
        assert!(Handle::new("foo.bar").is_ok());
        assert!(Handle::new("foo.9bar").is_err());
    }

    #[test]
    fn disallowed_tlds() {
        assert!(Handle::new("foo.local").is_err());
        assert!(Handle::new("foo.localhost").is_err());
        assert!(Handle::new("foo.arpa").is_err());
        assert!(Handle::new("foo.invalid").is_err());
        assert!(Handle::new("foo.internal").is_err());
        assert!(Handle::new("foo.example").is_err());
        assert!(Handle::new("foo.alt").is_err());
        assert!(Handle::new("foo.onion").is_err());
    }

    #[test]
    fn minimum_segments() {
        assert!(Handle::new("a.b").is_ok());
        assert!(Handle::new("a").is_err());
        assert!(Handle::new("com").is_err());
    }

    #[test]
    fn invalid_characters() {
        assert!(Handle::new("foo!bar.com").is_err());
        assert!(Handle::new("foo_bar.com").is_err());
        assert!(Handle::new("foo bar.com").is_err());
        assert!(Handle::new("foo@bar.com").is_err());
    }

    #[test]
    fn empty_segments() {
        assert!(Handle::new("foo..com").is_err());
        assert!(Handle::new(".foo.com").is_err());
        assert!(Handle::new("foo.com.").is_err());
    }
}
