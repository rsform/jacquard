use crate::types::ident::AtIdentifier;
use crate::types::nsid::Nsid;
use crate::types::recordkey::{RecordKey, Rkey};
use crate::types::string::AtStrError;
use crate::{CowStr, IntoStatic};
use regex::Regex;
use serde::Serializer;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

/// AT Protocol URI (`at://`) for referencing records in repositories
///
/// AT URIs provide a way to reference records using either a DID or handle as the authority.
/// They're not content-addressed, so the record's contents can change over time.
///
/// Format: `at://AUTHORITY[/COLLECTION[/RKEY]][#FRAGMENT]`
/// - Authority: DID or handle identifying the repository (required)
/// - Collection: NSID of the record type (optional)
/// - Record key (rkey): specific record identifier (optional)
/// - Fragment: sub-resource identifier (optional, limited support)
///
/// Examples:
/// - `at://alice.bsky.social`
/// - `at://did:plc:abc123/app.bsky.feed.post/3jk5`
///
/// See: <https://atproto.com/specs/at-uri-scheme>
#[derive(PartialEq, Eq, Debug)]
pub struct AtUri<'u> {
    inner: Inner<'u>,
}

#[ouroboros::self_referencing]
#[derive(PartialEq, Eq, Debug)]
struct Inner<'u> {
    uri: CowStr<'u>,
    #[borrows(uri)]
    #[covariant]
    pub authority: AtIdentifier<'this>,
    #[borrows(uri)]
    #[covariant]
    pub path: Option<RepoPath<'this>>,
    #[borrows(uri)]
    #[covariant]
    pub fragment: Option<CowStr<'this>>,
}

impl Clone for AtUri<'_> {
    fn clone(&self) -> Self {
        let uri = self.inner.borrow_uri();

        Self {
            inner: Inner::new(
                CowStr::Owned(uri.as_ref().to_smolstr()),
                |uri| {
                    let parts = ATURI_REGEX.captures(uri).unwrap();
                    unsafe { AtIdentifier::unchecked(parts.name("authority").unwrap().as_str()) }
                },
                |uri| {
                    let parts = ATURI_REGEX.captures(uri).unwrap();
                    if let Some(collection) = parts.name("collection") {
                        let collection = unsafe { Nsid::unchecked(collection.as_str()) };
                        let rkey = if let Some(rkey) = parts.name("rkey") {
                            let rkey = unsafe { RecordKey::from(Rkey::unchecked(rkey.as_str())) };
                            Some(rkey)
                        } else {
                            None
                        };
                        Some(RepoPath { collection, rkey })
                    } else {
                        None
                    }
                },
                |uri| {
                    let parts = ATURI_REGEX.captures(uri).unwrap();
                    parts.name("fragment").map(|fragment| {
                        let fragment = CowStr::Borrowed(fragment.as_str());
                        fragment
                    })
                },
            ),
        }
    }
}

impl Hash for AtUri<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.borrow_uri().hash(state);
    }
}

/// Path component of an AT URI (collection and optional record key)
///
/// Represents the `/COLLECTION[/RKEY]` portion of an AT URI.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RepoPath<'u> {
    /// Collection NSID (e.g., `app.bsky.feed.post`)
    pub collection: Nsid<'u>,
    /// Optional record key identifying a specific record
    pub rkey: Option<RecordKey<Rkey<'u>>>,
}

impl IntoStatic for RepoPath<'_> {
    type Output = RepoPath<'static>;

    fn into_static(self) -> Self::Output {
        RepoPath {
            collection: self.collection.into_static(),
            rkey: self.rkey.map(|rkey| rkey.into_static()),
        }
    }
}

/// Owned (static lifetime) version of `RepoPath`
pub type UriPathBuf = RepoPath<'static>;

/// Regex for AT URI validation per AT Protocol spec
pub static ATURI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Fragment allows: / and \ and other special chars. In raw string, backslashes are literal.
    Regex::new(r##"^at://(?<authority>[a-zA-Z0-9._:%-]+)(/(?<collection>[a-zA-Z0-9-.]+)(/(?<rkey>[a-zA-Z0-9._~:@!$&%')(*+,;=-]+))?)?(#(?<fragment>/[a-zA-Z0-9._~:@!$&%')(*+,;=\-\[\]/\\]*))?$"##).unwrap()
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
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Borrowed(fragment.as_str());
                    fragment
                });
                Ok(AtUri {
                    inner: InnerBuilder {
                        uri: CowStr::Borrowed(uri),
                        authority_builder: |_| authority,
                        path_builder: |_| path,
                        fragment_builder: |_| fragment,
                    }
                    .build(),
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

    /// Infallible constructor for when you know the URI is valid
    ///
    /// Panics on invalid URIs. Use this when manually constructing URIs from trusted sources.
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
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Borrowed(fragment.as_str());
                    fragment
                });
                AtUri {
                    inner: InnerBuilder {
                        uri: CowStr::Borrowed(uri),
                        authority_builder: |_| authority,
                        path_builder: |_| path,
                        fragment_builder: |_| fragment,
                    }
                    .build(),
                }
            } else {
                panic!("at:// URI missing authority")
            }
        } else {
            panic!("Invalid at:// URI via regex")
        }
    }

    /// Unchecked borrowing constructor. This one does do some validation but if that fails will just
    /// dump everything in the authority field.
    ///
    /// TODO: do some fallback splitting, but really, if you use this on something invalid, you deserve it.
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
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::Borrowed(fragment.as_str());
                    fragment
                });
                AtUri {
                    inner: InnerBuilder {
                        uri: CowStr::Borrowed(uri),
                        authority_builder: |_| authority,
                        path_builder: |_| path,
                        fragment_builder: |_| fragment,
                    }
                    .build(),
                }
            } else {
                // let mut uriParts = uri.split('#');
                // let mut parts = uriParts.next().unwrap_or(uri).split('/');
                // let auth = parts.next().unwrap_or(uri);
                Self {
                    inner: InnerBuilder {
                        uri: CowStr::Borrowed(uri),
                        authority_builder: |_| unsafe { AtIdentifier::unchecked(uri) },
                        path_builder: |_| None,
                        fragment_builder: |_| None,
                    }
                    .build(),
                }
            }
        } else {
            Self {
                inner: InnerBuilder {
                    uri: CowStr::Borrowed(uri),
                    authority_builder: |_| unsafe { AtIdentifier::unchecked(uri) },
                    path_builder: |_| None,
                    fragment_builder: |_| None,
                }
                .build(),
            }
        }
    }

    /// Clone method that should be O(1) in terms of time
    ///
    /// Calling on a borrowed variant will turn it into an owned variant, taking a little
    /// more time and allocating memory for each part. Calling it on an owned variant will
    /// increment all the internal reference counters (or, if constructed from a `&'static str`,
    /// essentially do nothing).
    pub fn fast_clone(&self) -> AtUri<'static> {
        self.inner.with(move |u| {
            let uri = u.uri.clone().into_static();
            let authority = u.authority.clone().into_static();
            let path = u.path.clone().into_static();
            let fragment = u.fragment.clone().into_static();
            AtUri {
                inner: InnerBuilder {
                    uri,
                    authority_builder: |_| authority,
                    path_builder: |_| path,
                    fragment_builder: |_| fragment,
                }
                .build(),
            }
        })
    }

    /// Get the full URI as a string slice
    pub fn as_str(&self) -> &str {
        {
            let this = &self.inner.borrow_uri();
            this
        }
    }

    /// Get the authority component (DID or handle)
    pub fn authority(&self) -> &AtIdentifier<'_> {
        self.inner.borrow_authority()
    }

    /// Get the path component (collection and optional rkey)
    pub fn path(&self) -> &Option<RepoPath<'_>> {
        self.inner.borrow_path()
    }

    /// Get the fragment component if present
    pub fn fragment(&self) -> &Option<CowStr<'_>> {
        self.inner.borrow_fragment()
    }

    /// Get the collection NSID from the path, if present
    pub fn collection(&self) -> Option<&Nsid<'_>> {
        self.inner.borrow_path().as_ref().map(|p| &p.collection)
    }

    /// Get the record key from the path, if present
    pub fn rkey(&self) -> Option<&RecordKey<Rkey<'_>>> {
        self.inner
            .borrow_path()
            .as_ref()
            .and_then(|p| p.rkey.as_ref())
    }
}

impl AtUri<'static> {
    /// Owned constructor
    ///
    /// Uses ouroboros self-referential tricks internally to make sure everything
    /// borrows efficiently from the uri `CowStr<'static>`.
    ///
    /// Performs validation up-front, but is slower than the borrowing constructor
    /// due to currently having to re-run the main regex, in addition to allocating.
    ///
    /// `.into_static()` and Clone implementations have similar limitations.
    ///
    /// O(1) clone mathod is AtUri::fast_clone().
    ///
    /// Future optimization involves working out the indices borrowed and either using those
    /// to avoid re-computing in some places, or, for a likely fully optimal version, only storing
    /// the indices and constructing the borrowed components unsafely when asked.
    pub fn new_owned(uri: impl AsRef<str>) -> Result<Self, AtStrError> {
        if let Some(parts) = ATURI_REGEX.captures(uri.as_ref()) {
            if let Some(authority) = parts.name("authority") {
                let _authority = AtIdentifier::new(authority.as_str())
                    .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.as_ref().to_string(), e))?;
                let path = if let Some(collection) = parts.name("collection") {
                    let collection = Nsid::new(collection.as_str()).map_err(|e| {
                        AtStrError::wrap("at-uri-scheme", uri.as_ref().to_string(), e)
                    })?;
                    let rkey = if let Some(rkey) = parts.name("rkey") {
                        let rkey = RecordKey::from(Rkey::new(rkey.as_str()).map_err(|e| {
                            AtStrError::wrap("at-uri-scheme", uri.as_ref().to_string(), e)
                        })?);
                        Some(rkey)
                    } else {
                        None
                    };
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };

                Ok(AtUri {
                    inner: Inner::new(
                        CowStr::Owned(uri.as_ref().to_smolstr()),
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            unsafe {
                                AtIdentifier::unchecked(parts.name("authority").unwrap().as_str())
                            }
                        },
                        |uri| {
                            if path.is_some() {
                                let parts = ATURI_REGEX.captures(uri).unwrap();
                                if let Some(collection) = parts.name("collection") {
                                    let collection =
                                        unsafe { Nsid::unchecked(collection.as_str()) };
                                    let rkey = if let Some(rkey) = parts.name("rkey") {
                                        let rkey = unsafe {
                                            RecordKey::from(Rkey::unchecked(rkey.as_str()))
                                        };
                                        Some(rkey)
                                    } else {
                                        None
                                    };
                                    Some(RepoPath { collection, rkey })
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        },
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            parts.name("fragment").map(|fragment| {
                                let fragment = CowStr::Borrowed(fragment.as_str());
                                fragment
                            })
                        },
                    ),
                })
            } else {
                Err(AtStrError::missing(
                    "at-uri-scheme",
                    &uri.as_ref(),
                    "authority",
                ))
            }
        } else {
            Err(AtStrError::regex(
                "at-uri-scheme",
                &uri.as_ref(),
                SmolStr::new_static("doesn't match schema"),
            ))
        }
    }

    /// Fallible constructor, validates, doesn't allocate (static lifetime)
    pub fn new_static(uri: &'static str) -> Result<Self, AtStrError> {
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
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };
                let fragment = parts.name("fragment").map(|fragment| {
                    let fragment = CowStr::new_static(fragment.as_str());
                    fragment
                });
                Ok(AtUri {
                    inner: InnerBuilder {
                        uri: CowStr::new_static(uri),
                        authority_builder: |_| authority,
                        path_builder: |_| path,
                        fragment_builder: |_| fragment,
                    }
                    .build(),
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
}

impl FromStr for AtUri<'_> {
    type Err = AtStrError;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `AtUri::new()` or `AtUri::raw()` if you want to borrow.
    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        if let Some(parts) = ATURI_REGEX.captures(uri.as_ref()) {
            if let Some(authority) = parts.name("authority") {
                let _authority = AtIdentifier::new(authority.as_str())
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
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };

                Ok(AtUri {
                    inner: Inner::new(
                        CowStr::Owned(uri.to_smolstr()),
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            unsafe {
                                AtIdentifier::unchecked(parts.name("authority").unwrap().as_str())
                            }
                        },
                        |uri| {
                            if path.is_some() {
                                let parts = ATURI_REGEX.captures(uri).unwrap();
                                if let Some(collection) = parts.name("collection") {
                                    let collection =
                                        unsafe { Nsid::unchecked(collection.as_str()) };
                                    let rkey = if let Some(rkey) = parts.name("rkey") {
                                        let rkey = unsafe {
                                            RecordKey::from(Rkey::unchecked(rkey.as_str()))
                                        };
                                        Some(rkey)
                                    } else {
                                        None
                                    };
                                    Some(RepoPath { collection, rkey })
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        },
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            parts.name("fragment").map(|fragment| {
                                let fragment = CowStr::Borrowed(fragment.as_str());
                                fragment
                            })
                        },
                    ),
                })
            } else {
                Err(AtStrError::missing(
                    "at-uri-scheme",
                    &uri.as_ref(),
                    "authority",
                ))
            }
        } else {
            Err(AtStrError::regex(
                "at-uri-scheme",
                &uri.as_ref(),
                SmolStr::new_static("doesn't match schema"),
            ))
        }
    }
}

impl IntoStatic for AtUri<'_> {
    type Output = AtUri<'static>;

    fn into_static(self) -> AtUri<'static> {
        AtUri {
            inner: Inner::new(
                self.inner.borrow_uri().clone().into_static(),
                |uri| {
                    let parts = ATURI_REGEX.captures(uri).unwrap();
                    unsafe { AtIdentifier::unchecked(parts.name("authority").unwrap().as_str()) }
                },
                |uri| {
                    if self.inner.borrow_path().is_some() {
                        let parts = ATURI_REGEX.captures(uri).unwrap();
                        if let Some(collection) = parts.name("collection") {
                            let collection = unsafe { Nsid::unchecked(collection.as_str()) };
                            let rkey = if let Some(rkey) = parts.name("rkey") {
                                let rkey =
                                    unsafe { RecordKey::from(Rkey::unchecked(rkey.as_str())) };
                                Some(rkey)
                            } else {
                                None
                            };
                            Some(RepoPath { collection, rkey })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                },
                |uri| {
                    if self.inner.borrow_fragment().is_some() {
                        let parts = ATURI_REGEX.captures(uri).unwrap();
                        parts.name("fragment").map(|fragment| {
                            let fragment = CowStr::Borrowed(fragment.as_str());
                            fragment
                        })
                    } else {
                        None
                    }
                },
            ),
        }
    }
}

impl<'de, 'a> Deserialize<'de> for AtUri<'a>
where
    'de: 'a,
{
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
        serializer.serialize_str(&self.inner.borrow_uri())
    }
}

impl fmt::Display for AtUri<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.borrow_uri())
    }
}

impl<'d> From<AtUri<'d>> for String {
    fn from(value: AtUri<'d>) -> Self {
        value.inner.borrow_uri().to_string()
    }
}

impl<'d> From<AtUri<'d>> for CowStr<'d> {
    fn from(value: AtUri<'d>) -> Self {
        value.inner.borrow_uri().clone()
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
    fn try_from(uri: CowStr<'d>) -> Result<Self, Self::Error> {
        if let Some(parts) = ATURI_REGEX.captures(uri.as_ref()) {
            if let Some(authority) = parts.name("authority") {
                let _authority = AtIdentifier::new(authority.as_str())
                    .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
                let _path = if let Some(collection) = parts.name("collection") {
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
                    Some(RepoPath { collection, rkey })
                } else {
                    None
                };
                drop(parts);

                Ok(AtUri {
                    inner: Inner::new(
                        uri,
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            unsafe {
                                AtIdentifier::unchecked(parts.name("authority").unwrap().as_str())
                            }
                        },
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            if let Some(collection) = parts.name("collection") {
                                let collection = unsafe { Nsid::unchecked(collection.as_str()) };
                                let rkey = if let Some(rkey) = parts.name("rkey") {
                                    let rkey =
                                        unsafe { RecordKey::from(Rkey::unchecked(rkey.as_str())) };
                                    Some(rkey)
                                } else {
                                    None
                                };
                                Some(RepoPath { collection, rkey })
                            } else {
                                None
                            }
                        },
                        |uri| {
                            let parts = ATURI_REGEX.captures(uri).unwrap();
                            parts.name("fragment").map(|fragment| {
                                let fragment = CowStr::Borrowed(fragment.as_str());
                                fragment
                            })
                        },
                    ),
                })
            } else {
                Err(AtStrError::missing(
                    "at-uri-scheme",
                    &uri.as_ref(),
                    "authority",
                ))
            }
        } else {
            Err(AtStrError::regex(
                "at-uri-scheme",
                &uri.as_ref(),
                SmolStr::new_static("doesn't match schema"),
            ))
        }
    }
}

impl AsRef<str> for AtUri<'_> {
    fn as_ref(&self) -> &str {
        &self.inner.borrow_uri().as_ref()
    }
}

impl Deref for AtUri<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.inner.borrow_uri().as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_at_uris() {
        assert!(AtUri::new("at://did:plc:foo").is_ok());
        assert!(AtUri::new("at://alice.bsky.social").is_ok());
        assert!(AtUri::new("at://did:plc:foo/com.example.post").is_ok());
        assert!(AtUri::new("at://did:plc:foo/com.example.post/123").is_ok());
    }

    #[test]
    fn authority_only() {
        let uri = AtUri::new("at://alice.test").unwrap();
        assert_eq!(uri.authority().as_str(), "alice.test");
        assert!(uri.collection().is_none());
        assert!(uri.rkey().is_none());
    }

    #[test]
    fn authority_and_collection() {
        let uri = AtUri::new("at://alice.test/com.example.foo").unwrap();
        assert_eq!(uri.authority().as_str(), "alice.test");
        assert_eq!(uri.collection().unwrap().as_str(), "com.example.foo");
        assert!(uri.rkey().is_none());
    }

    #[test]
    fn full_uri() {
        let uri = AtUri::new("at://alice.test/com.example.foo/123").unwrap();
        assert_eq!(uri.authority().as_str(), "alice.test");
        assert_eq!(uri.collection().unwrap().as_str(), "com.example.foo");
        assert_eq!(uri.rkey().unwrap().as_ref(), "123");
    }

    #[test]
    fn with_fragment() {
        let uri = AtUri::new("at://alice.test/com.example.foo/123#/path").unwrap();
        assert_eq!(uri.fragment().as_ref().unwrap().as_ref(), "/path");

        // Fragment must start with /
        assert!(AtUri::new("at://alice.test#path").is_err());
        assert!(AtUri::new("at://alice.test#/foo/bar").is_ok());
    }

    #[test]
    fn no_trailing_slash() {
        assert!(AtUri::new("at://alice.test/").is_err());
        assert!(AtUri::new("at://alice.test/com.example.foo/").is_err());
    }

    #[test]
    fn must_have_authority() {
        assert!(AtUri::new("at://").is_err());
        assert!(AtUri::new("at:///com.example.foo").is_err());
    }

    #[test]
    fn must_start_with_at_scheme() {
        assert!(AtUri::new("alice.test").is_err());
        assert!(AtUri::new("https://alice.test").is_err());
    }

    #[test]
    fn max_length() {
        // Spec says 8KB max
        let long_did = format!("did:plc:{}", "a".repeat(8000));
        let uri = format!("at://{}", long_did);
        assert!(uri.len() < 8192);
        // Should work if components are valid
        // (our DID will fail at 2048 chars, but this tests the URI doesn't impose extra limits)
    }
}
