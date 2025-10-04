use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::{ops::Deref, str::FromStr};

use crate::CowStr;

/// An IETF language tag.
///
/// Uses langtag crate for validation, but is stored as a SmolStr for size/avoiding allocations
///
/// TODO: Implement langtag-style semantic matching for this type, delegating to langtag
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Language(SmolStr);

impl Language {
    /// Parses an IETF language tag from the given string.
    pub fn new<T>(lang: &T) -> Result<Self, langtag::InvalidLangTag<&T>>
    where
        T: AsRef<str> + ?Sized,
    {
        let tag = langtag::LangTag::new(lang)?;
        Ok(Language(SmolStr::new(tag.as_str())))
    }

    /// Parses an IETF language tag from a static string.
    pub fn new_static(lang: &'static str) -> Result<Self, langtag::InvalidLangTag<&'static str>> {
        let tag = langtag::LangTag::new(lang)?;
        Ok(Language(SmolStr::new_static(tag.as_str())))
    }

    /// Infallible constructor for when you *know* the string is a valid IETF language tag.
    /// Will panic on invalid tag. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The `From<String>` and `From<CowStr>` impls use the same logic.
    pub fn raw(lang: impl AsRef<str>) -> Self {
        let lang = lang.as_ref();
        let tag = langtag::LangTag::new(lang).expect("valid IETF language tag");
        Language(SmolStr::new(tag.as_str()))
    }

    /// Infallible constructor for when you *know* the string is a valid IETF language tag.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(lang: impl AsRef<str>) -> Self {
        let lang = lang.as_ref();
        Self(SmolStr::new(lang))
    }

    /// Returns the LANG as a string slice.
    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Language {
    type Err = SmolStr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s).map_err(|e| e.0.to_smolstr())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Language> for String {
    fn from(value: Language) -> Self {
        value.0.to_string()
    }
}

impl From<Language> for SmolStr {
    fn from(value: Language) -> Self {
        value.0
    }
}

impl From<String> for Language {
    fn from(value: String) -> Self {
        Self::raw(&value)
    }
}

impl<'t> From<CowStr<'t>> for Language {
    fn from(value: CowStr<'t>) -> Self {
        Self::raw(&value)
    }
}

impl AsRef<str> for Language {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Language {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl crate::IntoStatic for Language {
    type Output = Language;

    fn into_static(self) -> Self::Output {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_language_tags() {
        assert!(Language::new("en").is_ok());
        assert!(Language::new("en-US").is_ok());
        assert!(Language::new("zh-Hans").is_ok());
        assert!(Language::new("es-419").is_ok());
    }

    #[test]
    fn case_insensitive_but_preserves() {
        let lang = Language::new("en-US").unwrap();
        assert_eq!(lang.as_str(), "en-US");
    }

    #[test]
    fn invalid_tags() {
        assert!(Language::new("").is_err());
        assert!(Language::new("not_a_tag").is_err());
        assert!(Language::new("123").is_err());
    }
}
