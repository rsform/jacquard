use crate::CowStr;
use crate::types::ident::AtIdentifier;
use crate::types::nsid::Nsid;
use crate::types::recordkey::{RecordKey, Rkey};
use crate::types::string::AtStrError;
use regex::Regex;
use serde::Serializer;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

/// at:// URI type
///
/// based on the regex here: https://github.com/bluesky-social/atproto/blob/main/packages/syntax/src/aturi_validation.ts
///
/// Doesn't support the query segment, but then neither does the Typescript SDK
///
/// TODO: support IntoStatic on string types. For composites like this where all borrow from (present) input,
///       perhaps use some careful unsafe to launder the lifetimes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AtUri<'u> {
    uri: CowStr<'u>,
    pub authority: AtIdentifier<'u>,
    pub path: Option<UriPath<'u>>,
    pub fragment: Option<CowStr<'u>>,
}

/// at:// URI path component (current subset)
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UriPath<'u> {
    pub collection: Nsid<'u>,
    pub rkey: Option<RecordKey<Rkey<'u>>>,
}

pub type UriPathBuf = UriPath<'static>;

pub static ATURI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"^at://(?<authority>[a-zA-Z0-9._:%-]+)(/(?<collection>[a-zA-Z0-9-.]+)(/(?<rkey>[a-zA-Z0-9._~:@!$&%')(*+,;=-]+))?)?(#(?<fragment>/[a-zA-Z0-9._~:@!$&%')(*+,;=-[]/\]*))?$"##).unwrap()
});

impl<'u> AtUri<'u> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(uri: &'u str) -> Result<Self, AtStrError> {
        if let Some(parts) = ATURI_REGEX.captures(uri) {
            if let Some(authority) = parts.name("authority") {
                let authority = AtIdentifier::new(authority.as_str())
                    .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                let path = if let Some(collection) = parts.name("collection") {
                    let collection = Nsid::new(collection.as_str())
                        .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                    let rkey = if let Some(rkey) = parts.name("rkey") {
                        let rkey =
                            RecordKey::from(Rkey::new(rkey.as_str()).map_err(|e| {
                                AtStrError::wrap("at-uri-scheme", uri.to_string(), e)
                            })?);
                        Some(rkey)
                    } else {
                        None
                    };
                    Some(UriPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Borrowed(fragment.as_str());
                    fragment
                });
                Ok(AtUri {
                    uri: CowStr::Borrowed(uri),
                    authority,
                    path,
                    fragment,
                })
            } else {
                Err(AtStrError::missing("at-uri-scheme", uri, "authority"))
            }
        } else {
            Err(AtStrError::regex(
                "at-uri-scheme",
                uri,
                SmolStr::new_static("doesn't match schema"),
            ))
        }
    }

    pub fn raw(uri: &'u str) -> Self {
        if let Some(parts) = ATURI_REGEX.captures(uri) {
            if let Some(authority) = parts.name("authority") {
                let authority = AtIdentifier::raw(authority.as_str());
                let path = if let Some(collection) = parts.name("collection") {
                    let collection = Nsid::raw(collection.as_str());
                    let rkey = if let Some(rkey) = parts.name("rkey") {
                        let rkey = RecordKey::from(Rkey::raw(rkey.as_str()));
                        Some(rkey)
                    } else {
                        None
                    };
                    Some(UriPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Borrowed(fragment.as_str());
                    fragment
                });
                AtUri {
                    uri: CowStr::Borrowed(uri),
                    authority,
                    path,
                    fragment,
                }
            } else {
                panic!("at:// URI missing authority")
            }
        } else {
            panic!("Invalid at:// URI via regex")
        }
    }

    pub fn new_owned(uri: impl AsRef<str>) -> Result<Self, AtStrError> {
        let uri = uri.as_ref();
        if let Some(parts) = ATURI_REGEX.captures(uri) {
            if let Some(authority) = parts.name("authority") {
                let authority = AtIdentifier::new_owned(authority.as_str())
                    .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                let path = if let Some(collection) = parts.name("collection") {
                    let collection = Nsid::new_owned(collection.as_str())
                        .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                    let rkey = if let Some(rkey) = parts.name("rkey") {
                        let rkey =
                            RecordKey::from(Rkey::new_owned(rkey.as_str()).map_err(|e| {
                                AtStrError::wrap("at-uri-scheme", uri.to_string(), e)
                            })?);
                        Some(rkey)
                    } else {
                        None
                    };
                    Some(UriPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Owned(fragment.as_str().to_smolstr());
                    fragment
                });
                Ok(AtUri {
                    uri: CowStr::Owned(uri.to_smolstr()),
                    authority,
                    path,
                    fragment,
                })
            } else {
                Err(AtStrError::missing("at-uri-scheme", uri, "authority"))
            }
        } else {
            Err(AtStrError::regex(
                "at-uri-scheme",
                uri,
                SmolStr::new_static("doesn't match schema"),
            ))
        }
    }

    pub fn new_static(uri: &'static str) -> Result<AtUri<'static>, AtStrError> {
        let uri = uri.as_ref();
        if let Some(parts) = ATURI_REGEX.captures(uri) {
            if let Some(authority) = parts.name("authority") {
                let authority = AtIdentifier::new_static(authority.as_str())
                    .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                let path = if let Some(collection) = parts.name("collection") {
                    let collection = Nsid::new_static(collection.as_str())
                        .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                    let rkey = if let Some(rkey) = parts.name("rkey") {
                        let rkey =
                            RecordKey::from(Rkey::new_static(rkey.as_str()).map_err(|e| {
                                AtStrError::wrap("at-uri-scheme", uri.to_string(), e)
                            })?);
                        Some(rkey)
                    } else {
                        None
                    };
                    Some(UriPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::new_static(fragment.as_str());
                    fragment
                });
                Ok(AtUri {
                    uri: CowStr::new_static(uri),
                    authority,
                    path,
                    fragment,
                })
            } else {
                Err(AtStrError::missing("at-uri-scheme", uri, "authority"))
            }
        } else {
            Err(AtStrError::regex(
                "at-uri-scheme",
                uri,
                SmolStr::new_static("doesn't match schema"),
            ))
        }
    }

    pub unsafe fn unchecked(uri: &'u str) -> Self {
        if let Some(parts) = ATURI_REGEX.captures(uri) {
            if let Some(authority) = parts.name("authority") {
                let authority = unsafe { AtIdentifier::unchecked(authority.as_str()) };
                let path = if let Some(collection) = parts.name("collection") {
                    let collection = unsafe { Nsid::unchecked(collection.as_str()) };
                    let rkey = if let Some(rkey) = parts.name("rkey") {
                        let rkey = RecordKey::from(unsafe { Rkey::unchecked(rkey.as_str()) });
                        Some(rkey)
                    } else {
                        None
                    };
                    Some(UriPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Borrowed(fragment.as_str());
                    fragment
                });
                AtUri {
                    uri: CowStr::Borrowed(uri),
                    authority,
                    path,
                    fragment,
                }
            } else {
                Self {
                    uri: CowStr::Borrowed(uri),
                    authority: unsafe { AtIdentifier::unchecked(uri) },
                    path: None,
                    fragment: None,
                }
            }
        } else {
            Self {
                uri: CowStr::Borrowed(uri),
                authority: unsafe { AtIdentifier::unchecked(uri) },
                path: None,
                fragment: None,
            }
        }
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.uri;
            this
        }
    }
}

impl FromStr for AtUri<'_> {
    type Err = AtStrError;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `AtUri::new()` or `AtUri::raw()` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<'de> Deserialize<'de> for AtUri<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Serialize for AtUri<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.uri)
    }
}

impl fmt::Display for AtUri<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.uri)
    }
}

impl<'d> From<AtUri<'d>> for String {
    fn from(value: AtUri<'d>) -> Self {
        value.uri.to_string()
    }
}

impl<'d> From<AtUri<'d>> for CowStr<'d> {
    fn from(value: AtUri<'d>) -> Self {
        value.uri
    }
}

impl TryFrom<String> for AtUri<'static> {
    type Error = AtStrError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new_owned(&value)
    }
}

impl<'d> TryFrom<CowStr<'d>> for AtUri<'d> {
    type Error = AtStrError;
    /// TODO: rewrite to avoid taking ownership/cloning
    fn try_from(value: CowStr<'d>) -> Result<Self, Self::Error> {
        Self::new_owned(value)
    }
}

impl AsRef<str> for AtUri<'_> {
    fn as_ref(&self) -> &str {
        &self.uri.as_ref()
    }
}

impl Deref for AtUri<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.uri.as_ref()
    }
}
