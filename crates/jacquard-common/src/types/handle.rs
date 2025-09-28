use crate::{CowStr, IntoStatic};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::ToSmolStr;
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
    /// Accepts (and strips) preceding '@' if present
    pub fn new(handle: &'h str) -> Result<Self, &'static str> {
        let handle = handle
            .strip_prefix("at://")
            .unwrap_or(handle)
            .strip_prefix('@')
            .unwrap_or(handle);
        if handle.len() > 253 {
            Err("handle too long")
        } else if !HANDLE_REGEX.is_match(handle) {
            Err("Invalid handle")
        } else {
            Ok(Self(CowStr::Borrowed(handle)))
        }
    }

    /// Fallible constructor, validates, takes ownership
    pub fn new_owned(handle: impl AsRef<str>) -> Result<Self, &'static str> {
        let handle = handle.as_ref();
        let handle = handle
            .strip_prefix("at://")
            .unwrap_or(handle)
            .strip_prefix('@')
            .unwrap_or(handle);
        if handle.len() > 253 {
            Err("handle too long")
        } else if !HANDLE_REGEX.is_match(handle) {
            Err("Invalid handle")
        } else {
            Ok(Self(CowStr::Owned(handle.to_smolstr())))
        }
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(handle: &'static str) -> Result<Self, &'static str> {
        let handle = handle
            .strip_prefix("at://")
            .unwrap_or(handle)
            .strip_prefix('@')
            .unwrap_or(handle);
        if handle.len() > 253 {
            Err("handle too long")
        } else if !HANDLE_REGEX.is_match(handle) {
            Err("Invalid handle")
        } else {
            Ok(Self(CowStr::new_static(handle)))
        }
    }
    /// Infallible constructor for when you *know* the string is a valid handle.
    /// Will panic on invalid handles. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    ///
    /// Accepts (and strips) preceding '@' if present
    pub fn raw(handle: &'h str) -> Self {
        let handle = handle
            .strip_prefix("at://")
            .unwrap_or(handle)
            .strip_prefix('@')
            .unwrap_or(handle);
        if handle.len() > 253 {
            panic!("handle too long")
        } else if !HANDLE_REGEX.is_match(handle) {
            panic!("Invalid handle")
        } else {
            Self(CowStr::Borrowed(handle))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid handle.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    ///
    /// Accepts (and strips) preceding '@' if present
    pub unsafe fn unchecked(handle: &'h str) -> Self {
        let handle = handle
            .strip_prefix("at://")
            .unwrap_or(handle)
            .strip_prefix('@')
            .unwrap_or(handle);
        Self(CowStr::Borrowed(handle))
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Handle<'_> {
    type Err = &'static str;

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
        let value = if let Some(handle) = value
            .strip_prefix("at://")
            .unwrap_or(&value)
            .strip_prefix('@')
        {
            CowStr::Borrowed(handle)
        } else {
            value.into()
        };
        if value.len() > 253 {
            panic!("handle too long")
        } else if !HANDLE_REGEX.is_match(&value) {
            panic!("Invalid handle")
        } else {
            Self(value.into_static())
        }
    }
}

impl<'h> From<CowStr<'h>> for Handle<'h> {
    fn from(value: CowStr<'h>) -> Self {
        let value = if let Some(handle) = value
            .strip_prefix("at://")
            .unwrap_or(&value)
            .strip_prefix('@')
        {
            CowStr::Borrowed(handle)
        } else {
            value
        };
        if value.len() > 253 {
            panic!("handle too long")
        } else if !HANDLE_REGEX.is_match(&value) {
            panic!("Invalid handle")
        } else {
            Self(value.into_static())
        }
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
