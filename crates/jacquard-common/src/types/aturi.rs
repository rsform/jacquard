use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

use compact_str::ToCompactString;
use serde::{Deserialize, Deserializer, Serialize, de::Error};

use crate::{CowStr, IntoStatic};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
pub struct AtUri<'a>(CowStr<'a>);

pub static AT_URI_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^$").unwrap());

impl<'a> AtUri<'a> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(uri: &'a str) -> Result<Self, &'static str> {
        if uri.len() > 2048 {
            Err("AT_URI too long")
        } else if !AT_URI_REGEX.is_match(uri) {
            Err("Invalid AT_URI")
        } else {
            Ok(Self(CowStr::Borrowed(uri)))
        }
    }

    /// Fallible constructor from an existing CowStr, clones and takes
    pub fn from_cowstr(uri: CowStr<'a>) -> Result<AtUri<'a>, &'static str> {
        if uri.len() > 2048 {
            Err("AT_URI too long")
        } else if !AT_URI_REGEX.is_match(&uri) {
            Err("Invalid AT_URI")
        } else {
            Ok(Self(uri.into_static()))
        }
    }

    /// Infallible constructor for when you *know* the string slice is a valid at:// uri.
    /// Will panic on invalid URIs. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    pub fn raw(uri: &'a str) -> Self {
        if uri.len() > 2048 {
            panic!("AT_URI too long")
        } else if !AT_URI_REGEX.is_match(uri) {
            panic!("Invalid AT_URI")
        } else {
            Self(CowStr::Borrowed(uri))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid AT_URI.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(uri: &'a str) -> Self {
        Self(CowStr::Borrowed(uri))
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for AtUri<'_> {
    type Err = &'static str;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `AtUri::new()` or `AtUri::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_cowstr(CowStr::Owned(s.to_compact_string()))
    }
}

impl<'ae> Deserialize<'ae> for AtUri<'ae> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'ae>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for AtUri<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'a> From<AtUri<'a>> for String {
    fn from(value: AtUri<'a>) -> Self {
        value.0.to_string()
    }
}

impl<'s> From<&'s AtUri<'_>> for &'s str {
    fn from(value: &'s AtUri<'_>) -> Self {
        value.0.as_ref()
    }
}

impl<'a> From<AtUri<'a>> for CowStr<'a> {
    fn from(value: AtUri<'a>) -> Self {
        value.0
    }
}

impl From<String> for AtUri<'static> {
    fn from(value: String) -> Self {
        if value.len() > 2048 {
            panic!("AT_URI too long")
        } else if !AT_URI_REGEX.is_match(&value) {
            panic!("Invalid AT_URI")
        } else {
            Self(CowStr::Owned(value.to_compact_string()))
        }
    }
}

impl<'a> From<CowStr<'a>> for AtUri<'a> {
    fn from(value: CowStr<'a>) -> Self {
        if value.len() > 2048 {
            panic!("AT_URI too long")
        } else if !AT_URI_REGEX.is_match(&value) {
            panic!("Invalid AT_URI")
        } else {
            Self(value)
        }
    }
}

impl AsRef<str> for AtUri<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for AtUri<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
