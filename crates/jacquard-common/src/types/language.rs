use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::{ops::Deref, str::FromStr};

use crate::CowStr;

/// A [Timestamp Identifier].
///
/// [Timestamp Identifier]: https://atproto.com/specs/lang
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Lang(SmolStr);

impl Lang {
    /// Parses an IETF language tag from the given string.
    pub fn new<T>(lang: &T) -> Result<Self, langtag::InvalidLangTag<&T>>
    where
        T: AsRef<str> + ?Sized,
    {
        let tag = langtag::LangTag::new(lang)?;
        Ok(Lang(SmolStr::new_inline(tag.as_str())))
    }

    /// Infallible constructor for when you *know* the string is a valid IETF language tag.
    /// Will panic on invalid tag. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    pub fn raw(lang: impl AsRef<str>) -> Self {
        let lang = lang.as_ref();
        let tag = langtag::LangTag::new(lang).expect("valid IETF language tag");
        Lang(SmolStr::new_inline(tag.as_str()))
    }

    /// Infallible constructor for when you *know* the string is a valid IETF language tag.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(lang: impl AsRef<str>) -> Self {
        let lang = lang.as_ref();
        Self(SmolStr::new_inline(lang))
    }

    /// Returns the LANG as a string slice.
    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Lang {
    type Err = SmolStr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s).map_err(|e| e.0.to_smolstr())
    }
}

impl<'de> Deserialize<'de> for Lang {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Lang> for String {
    fn from(value: Lang) -> Self {
        value.0.to_string()
    }
}

impl From<Lang> for SmolStr {
    fn from(value: Lang) -> Self {
        value.0
    }
}

impl From<String> for Lang {
    fn from(value: String) -> Self {
        Self::raw(&value)
    }
}

impl<'t> From<CowStr<'t>> for Lang {
    fn from(value: CowStr<'t>) -> Self {
        Self::raw(&value)
    }
}

impl AsRef<str> for Lang {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Lang {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
