use crate::types::recordkey::RecordKeyType;
use crate::types::string::AtStrError;
use crate::{CowStr, IntoStatic};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

/// Namespaced Identifier (NSID)
///
/// Stored as SmolStr to ease lifetime issues and because, despite the fact that NSIDs *can* be 317 characters, most are quite short
/// TODO: consider if this should go back to CowStr, or be broken up into segments
#[derive(Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Nsid<'n>(CowStr<'n>);

pub static NSID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+(\.[a-zA-Z][a-zA-Z0-9]{0,62})$").unwrap()
});

impl<'n> Nsid<'n> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(nsid: &'n str) -> Result<Self, AtStrError> {
        if nsid.len() > 317 {
            Err(AtStrError::too_long("nsid", nsid, 317, nsid.len()))
        } else if !NSID_REGEX.is_match(nsid) {
            Err(AtStrError::regex(
                "nsid",
                nsid,
                SmolStr::new_static("invalid"),
            ))
        } else {
            Ok(Self(CowStr::Borrowed(nsid)))
        }
    }

    /// Fallible constructor, validates, borrows from input
    pub fn new_owned(nsid: impl AsRef<str>) -> Result<Self, AtStrError> {
        let nsid = nsid.as_ref();
        if nsid.len() > 317 {
            Err(AtStrError::too_long("nsid", nsid, 317, nsid.len()))
        } else if !NSID_REGEX.is_match(nsid) {
            Err(AtStrError::regex(
                "nsid",
                nsid,
                SmolStr::new_static("invalid"),
            ))
        } else {
            Ok(Self(CowStr::Owned(nsid.to_smolstr())))
        }
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(nsid: &'static str) -> Result<Self, AtStrError> {
        if nsid.len() > 317 {
            Err(AtStrError::too_long("nsid", nsid, 317, nsid.len()))
        } else if !NSID_REGEX.is_match(nsid) {
            Err(AtStrError::regex(
                "nsid",
                nsid,
                SmolStr::new_static("invalid"),
            ))
        } else {
            Ok(Self(CowStr::new_static(nsid)))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid NSID.
    /// Will panic on invalid NSIDs. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    pub fn raw(nsid: &'n str) -> Self {
        if nsid.len() > 317 {
            panic!("NSID too long")
        } else if !NSID_REGEX.is_match(nsid) {
            panic!("Invalid NSID")
        } else {
            Self(CowStr::Borrowed(nsid))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid NSID.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(nsid: &'n str) -> Self {
        Self(CowStr::Borrowed(nsid))
    }

    /// Returns the domain authority part of the NSID.
    pub fn domain_authority(&self) -> &str {
        let split = self.0.rfind('.').expect("enforced by constructor");
        &self.0[..split]
    }

    /// Returns the name segment of the NSID.
    pub fn name(&self) -> &str {
        let split = self.0.rfind('.').expect("enforced by constructor");
        &self.0[split + 1..]
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl<'n> FromStr for Nsid<'n> {
    type Err = AtStrError;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `Nsid::new()` or `Nsid::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl IntoStatic for Nsid<'_> {
    type Output = Nsid<'static>;

    fn into_static(self) -> Self::Output {
        Nsid(self.0.into_static())
    }
}

impl<'de, 'a> Deserialize<'de> for Nsid<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Nsid<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Nsid<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}", self.0)
    }
}

impl<'n> From<Nsid<'n>> for String {
    fn from(value: Nsid) -> Self {
        value.0.to_string()
    }
}

impl<'n> From<Nsid<'n>> for CowStr<'n> {
    fn from(value: Nsid<'n>) -> Self {
        value.0
    }
}

impl From<Nsid<'_>> for SmolStr {
    fn from(value: Nsid) -> Self {
        value.0.to_smolstr()
    }
}

impl<'n> From<String> for Nsid<'n> {
    fn from(value: String) -> Self {
        if value.len() > 317 {
            panic!("NSID too long")
        } else if !NSID_REGEX.is_match(&value) {
            panic!("Invalid NSID")
        } else {
            Self(CowStr::Owned(value.to_smolstr()))
        }
    }
}

impl<'n> From<CowStr<'n>> for Nsid<'n> {
    fn from(value: CowStr<'n>) -> Self {
        if value.len() > 317 {
            panic!("NSID too long")
        } else if !NSID_REGEX.is_match(&value) {
            panic!("Invalid NSID")
        } else {
            Self(value)
        }
    }
}

impl From<SmolStr> for Nsid<'_> {
    fn from(value: SmolStr) -> Self {
        if value.len() > 317 {
            panic!("NSID too long")
        } else if !NSID_REGEX.is_match(&value) {
            panic!("Invalid NSID")
        } else {
            Self(CowStr::Owned(value))
        }
    }
}

impl AsRef<str> for Nsid<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Nsid<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

unsafe impl RecordKeyType for Nsid<'_> {
    fn as_str(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_nsids() {
        assert!(Nsid::new("com.example.foo").is_ok());
        assert!(Nsid::new("com.example.fooBar").is_ok());
        assert!(Nsid::new("com.long-domain.foo").is_ok());
        assert!(Nsid::new("a.b.c").is_ok());
        assert!(Nsid::new("a1.b2.c3").is_ok());
    }

    #[test]
    fn minimum_segments() {
        assert!(Nsid::new("a.b.c").is_ok()); // 3 segments minimum
        assert!(Nsid::new("a.b").is_err());
        assert!(Nsid::new("a").is_err());
    }

    #[test]
    fn domain_and_name_parsing() {
        let nsid = Nsid::new("com.example.fooBar").unwrap();
        assert_eq!(nsid.domain_authority(), "com.example");
        assert_eq!(nsid.name(), "fooBar");
    }

    #[test]
    fn max_length() {
        // 317 chars: 63 + 63 + 63 + 63 + 63 = 315 + 4 dots + 1 = 320, too much
        // try: 63 + 63 + 63 + 63 + 62 = 314 + 4 dots = 318, still too much
        // try: 63 + 63 + 63 + 63 + 61 = 313 + 4 dots = 317
        let s1 = format!("a{}a", "b".repeat(61));
        let s2 = format!("c{}c", "d".repeat(61));
        let s3 = format!("e{}e", "f".repeat(61));
        let s4 = format!("g{}g", "h".repeat(61));
        let s5 = format!("i{}i", "j".repeat(59));
        let valid_317 = format!("{}.{}.{}.{}.{}", s1, s2, s3, s4, s5);
        assert_eq!(valid_317.len(), 317);
        assert!(Nsid::new(&valid_317).is_ok());

        let s5_long = format!("i{}i", "j".repeat(60));
        let too_long_318 = format!("{}.{}.{}.{}.{}", s1, s2, s3, s4, s5_long);
        assert_eq!(too_long_318.len(), 318);
        assert!(Nsid::new(&too_long_318).is_err());
    }

    #[test]
    fn segment_length() {
        let valid_63 = format!("{}.{}.foo", "a".repeat(63), "b".repeat(63));
        assert!(Nsid::new(&valid_63).is_ok());

        let too_long_64 = format!("{}.b.foo", "a".repeat(64));
        assert!(Nsid::new(&too_long_64).is_err());
    }

    #[test]
    fn first_segment_cannot_start_with_digit() {
        assert!(Nsid::new("com.example.foo").is_ok());
        assert!(Nsid::new("9com.example.foo").is_err());
    }

    #[test]
    fn name_segment_rules() {
        assert!(Nsid::new("com.example.foo").is_ok());
        assert!(Nsid::new("com.example.fooBar123").is_ok());
        assert!(Nsid::new("com.example.9foo").is_err()); // can't start with digit
        assert!(Nsid::new("com.example.foo-bar").is_err()); // no hyphens in name
    }

    #[test]
    fn domain_segment_rules() {
        assert!(Nsid::new("foo-bar.example.baz").is_ok());
        assert!(Nsid::new("foo.bar-baz.qux").is_ok());
        assert!(Nsid::new("-foo.bar.baz").is_err()); // can't start with hyphen
        assert!(Nsid::new("foo-.bar.baz").is_err()); // can't end with hyphen
    }

    #[test]
    fn case_sensitivity() {
        // Domain should be case-insensitive per spec (but not enforced in validation)
        // Name is case-sensitive
        assert!(Nsid::new("com.example.fooBar").is_ok());
        assert!(Nsid::new("com.example.FooBar").is_ok());
    }

    #[test]
    fn no_hyphens_in_name() {
        assert!(Nsid::new("com.example.foo").is_ok());
        assert!(Nsid::new("com.example.foo-bar").is_err());
        assert!(Nsid::new("com.example.fooBar").is_ok());
    }
}
