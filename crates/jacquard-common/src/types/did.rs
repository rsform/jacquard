use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

use compact_str::ToCompactString;
use serde::{Deserialize, Deserializer, Serialize, de::Error};

use crate::{CowStr, IntoStatic};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
pub struct Did<'d>(CowStr<'d>);

pub static DID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^did:[a-z]+:[a-zA-Z0-9._:%-]*[a-zA-Z0-9._-]$").unwrap());

impl<'d> Did<'d> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(did: &'d str) -> Result<Self, &'static str> {
        if did.len() > 2048 {
            Err("DID too long")
        } else if !DID_REGEX.is_match(did) {
            Err("Invalid DID")
        } else {
            Ok(Self(CowStr::Borrowed(did)))
        }
    }

    /// Fallible constructor from an existing CowStr, takes ownership
    pub fn from_cowstr(did: CowStr<'d>) -> Result<Did<'d>, &'static str> {
        if did.len() > 2048 {
            Err("DID too long")
        } else if !DID_REGEX.is_match(&did) {
            Err("Invalid DID")
        } else {
            Ok(Self(did.into_static()))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid DID.
    /// Will panic on invalid DIDs. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    pub fn raw(did: &'d str) -> Self {
        if did.len() > 2048 {
            panic!("DID too long")
        } else if !DID_REGEX.is_match(did) {
            panic!("Invalid DID")
        } else {
            Self(CowStr::Borrowed(did))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid DID.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(did: &'d str) -> Self {
        Self(CowStr::Borrowed(did))
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Did<'_> {
    type Err = &'static str;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `Did::new()` or `Did::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_cowstr(CowStr::Borrowed(s).into_static())
    }
}

impl<'de> Deserialize<'de> for Did<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Did<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'d> From<Did<'d>> for String {
    fn from(value: Did<'d>) -> Self {
        value.0.to_string()
    }
}

impl<'d> From<Did<'d>> for CowStr<'d> {
    fn from(value: Did<'d>) -> Self {
        value.0
    }
}

impl From<String> for Did<'static> {
    fn from(value: String) -> Self {
        if value.len() > 2048 {
            panic!("DID too long")
        } else if !DID_REGEX.is_match(&value) {
            panic!("Invalid DID")
        } else {
            Self(CowStr::Owned(value.to_compact_string()))
        }
    }
}

impl<'d> From<CowStr<'d>> for Did<'d> {
    fn from(value: CowStr<'d>) -> Self {
        if value.len() > 2048 {
            panic!("DID too long")
        } else if !DID_REGEX.is_match(&value) {
            panic!("Invalid DID")
        } else {
            Self(value)
        }
    }
}

impl AsRef<str> for Did<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Did<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
