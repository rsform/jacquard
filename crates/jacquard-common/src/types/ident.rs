use crate::types::handle::Handle;
use crate::{IntoStatic, types::did::Did};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::CowStr;

/// An AT Protocol identifier.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Hash)]
#[serde(untagged)]
pub enum AtIdentifier<'i> {
    #[serde(borrow)]
    Did(Did<'i>),
    Handle(Handle<'i>),
}

impl<'i> AtIdentifier<'i> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(ident: &'i str) -> Result<Self, &'static str> {
        if let Ok(did) = ident.parse() {
            Ok(AtIdentifier::Did(did))
        } else {
            ident.parse().map(AtIdentifier::Handle)
        }
    }

    /// Fallible constructor, validates, takes ownership
    pub fn new_owned(ident: impl AsRef<str>) -> Result<Self, &'static str> {
        let ident = ident.as_ref();
        if let Ok(did) = Did::new_owned(ident) {
            Ok(AtIdentifier::Did(did))
        } else {
            Ok(AtIdentifier::Handle(Handle::new_owned(ident)?))
        }
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(ident: &'static str) -> Result<AtIdentifier<'static>, &'static str> {
        if let Ok(did) = Did::new_static(ident) {
            Ok(AtIdentifier::Did(did))
        } else {
            Ok(AtIdentifier::Handle(Handle::new_static(ident)?))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid identifier.
    /// Will panic on invalid identifiers. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    pub fn raw(ident: &'i str) -> Self {
        if let Ok(did) = ident.parse() {
            AtIdentifier::Did(did)
        } else {
            ident
                .parse()
                .map(AtIdentifier::Handle)
                .expect("valid handle")
        }
    }

    /// Infallible constructor for when you *know* the string is a valid identifier.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    ///
    /// Will validate DIDs, but will treat anything else as a valid handle
    pub unsafe fn unchecked(ident: &'i str) -> Self {
        if let Ok(did) = ident.parse() {
            AtIdentifier::Did(did)
        } else {
            unsafe { AtIdentifier::Handle(Handle::unchecked(ident)) }
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            AtIdentifier::Did(did) => did.as_str(),
            AtIdentifier::Handle(handle) => handle.as_str(),
        }
    }
}

impl<'i> From<Did<'i>> for AtIdentifier<'i> {
    fn from(did: Did<'i>) -> Self {
        AtIdentifier::Did(did)
    }
}

impl<'i> From<Handle<'i>> for AtIdentifier<'i> {
    fn from(handle: Handle<'i>) -> Self {
        AtIdentifier::Handle(handle)
    }
}

impl FromStr for AtIdentifier<'_> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(did) = s.parse() {
            Ok(AtIdentifier::Did(did))
        } else {
            s.parse().map(AtIdentifier::Handle)
        }
    }
}

impl IntoStatic for AtIdentifier<'_> {
    type Output = AtIdentifier<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            AtIdentifier::Did(did) => AtIdentifier::Did(did.into_static()),
            AtIdentifier::Handle(handle) => AtIdentifier::Handle(handle.into_static()),
        }
    }
}

impl fmt::Display for AtIdentifier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtIdentifier::Did(did) => did.fmt(f),
            AtIdentifier::Handle(handle) => handle.fmt(f),
        }
    }
}

impl From<String> for AtIdentifier<'static> {
    fn from(value: String) -> Self {
        if let Ok(did) = value.parse() {
            AtIdentifier::Did(did)
        } else {
            value
                .parse()
                .map(AtIdentifier::Handle)
                .expect("valid handle")
        }
    }
}

impl<'i> From<CowStr<'i>> for AtIdentifier<'i> {
    fn from(value: CowStr<'i>) -> Self {
        if let Ok(did) = value.parse() {
            AtIdentifier::Did(did)
        } else {
            value
                .parse()
                .map(AtIdentifier::Handle)
                .expect("valid handle")
        }
    }
}

impl<'i> From<AtIdentifier<'i>> for String {
    fn from(value: AtIdentifier) -> Self {
        match value {
            AtIdentifier::Did(did) => did.into(),
            AtIdentifier::Handle(handle) => handle.into(),
        }
    }
}

impl AsRef<str> for AtIdentifier<'_> {
    fn as_ref(&self) -> &str {
        match self {
            AtIdentifier::Did(did) => did.as_ref(),
            AtIdentifier::Handle(handle) => handle.as_ref(),
        }
    }
}
