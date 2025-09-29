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

impl<'de> Deserialize<'de> for Nsid<'de> {
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
