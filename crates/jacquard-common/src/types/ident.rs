use crate::bos::{Bos, DefaultStr};
use crate::types::handle::Handle;
use crate::types::string::AtStrError;
use crate::{
    CowStr, IntoStatic,
    types::did::{Did, validate_did},
};
use alloc::string::String;
use alloc::string::ToString;
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// AT Protocol identifier (either a DID or handle).
///
/// Represents the union of DIDs and handles, which can both be used to identify
/// accounts in AT Protocol. DIDs are permanent identifiers, while handles are
/// human-friendly and can change.
///
/// Automatically determines whether a string is a DID or a handle during parsing.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Hash)]
#[serde(untagged)]
#[serde(bound(
    serialize = "S: Bos<str> + AsRef<str> + Serialize",
    deserialize = "S: Bos<str> + AsRef<str> + Deserialize<'de>"
))]
pub enum AtIdentifier<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// DID variant
    Did(Did<S>),
    /// Handle variant
    Handle(Handle<S>),
}

// ---------------------------------------------------------------------------
// Core methods
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> AtIdentifier<S> {
    /// Get the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        match self {
            AtIdentifier::Did(did) => did.as_str(),
            AtIdentifier::Handle(handle) => handle.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Generic construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> AtIdentifier<S> {
    /// Fallible constructor, validates, wraps the input directly.
    ///
    /// Tries DID first, then handle. Rejects uppercase handles — use
    /// `new_owned()` for case-insensitive construction.
    pub fn new(ident: S) -> Result<Self, AtStrError> {
        let s = ident.as_ref();
        if validate_did(s).is_ok() {
            Ok(AtIdentifier::Did(unsafe { Did::unchecked(ident) }))
        } else {
            Handle::new(ident).map(AtIdentifier::Handle)
        }
    }

    /// Infallible constructor. Panics on invalid identifiers.
    pub fn raw(ident: S) -> Self {
        Self::new(ident).expect("valid identifier")
    }

    /// Infallible unchecked constructor.
    ///
    /// # Safety
    ///
    /// Validates DIDs, treats anything else as a valid handle.
    pub unsafe fn unchecked(ident: S) -> Self {
        if validate_did(ident.as_ref()).is_ok() {
            AtIdentifier::Did(unsafe { Did::unchecked(ident) })
        } else {
            unsafe { AtIdentifier::Handle(Handle::unchecked(ident)) }
        }
    }
}

// ---------------------------------------------------------------------------
// Owned construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str> + FromStr> AtIdentifier<S> {
    /// Fallible constructor, validates, takes ownership.
    /// Strips prefixes and normalises handle case.
    pub fn new_owned(ident: impl AsRef<str>) -> Result<Self, AtStrError> {
        let ident = ident.as_ref();
        if let Ok(did) = Did::new_owned(ident) {
            Ok(AtIdentifier::Did(did))
        } else {
            Handle::new_owned(ident).map(AtIdentifier::Handle)
        }
    }

    /// Fallible constructor for static strings.
    pub fn new_static(ident: &'static str) -> Result<Self, AtStrError> {
        if let Ok(did) = Did::new_static(ident) {
            Ok(AtIdentifier::Did(did))
        } else {
            Handle::new_static(ident).map(AtIdentifier::Handle)
        }
    }
}

// ---------------------------------------------------------------------------
// Trait impls
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for AtIdentifier<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = AtIdentifier<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            AtIdentifier::Did(did) => AtIdentifier::Did(did.into_static()),
            AtIdentifier::Handle(handle) => AtIdentifier::Handle(handle.into_static()),
        }
    }
}

impl<S: Bos<str> + AsRef<str>> AtIdentifier<S> {
    /// Convert to an `AtIdentifier` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> AtIdentifier<B> {
        match self {
            AtIdentifier::Did(did) => AtIdentifier::Did(did.convert()),
            AtIdentifier::Handle(handle) => AtIdentifier::Handle(handle.convert()),
        }
    }
}

impl<S: Bos<str> + AsRef<str>> From<Did<S>> for AtIdentifier<S> {
    fn from(did: Did<S>) -> Self {
        AtIdentifier::Did(did)
    }
}

impl<S: Bos<str> + AsRef<str>> From<Handle<S>> for AtIdentifier<S> {
    fn from(handle: Handle<S>) -> Self {
        AtIdentifier::Handle(handle)
    }
}

impl FromStr for AtIdentifier {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for AtIdentifier<CowStr<'static>> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for AtIdentifier<String> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for AtIdentifier<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtIdentifier::Did(did) => did.fmt(f),
            AtIdentifier::Handle(handle) => handle.fmt(f),
        }
    }
}

impl From<String> for AtIdentifier {
    fn from(value: String) -> Self {
        Self::new_owned(value).expect("valid identifier")
    }
}

impl<'i> From<CowStr<'i>> for AtIdentifier<CowStr<'i>> {
    fn from(value: CowStr<'i>) -> Self {
        Self::new(value).expect("valid identifier")
    }
}

impl<S: Bos<str> + AsRef<str>> From<AtIdentifier<S>> for String {
    fn from(value: AtIdentifier<S>) -> Self {
        value.as_str().to_string()
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for AtIdentifier<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use smol_str::SmolStr;

    use super::*;
    use crate::cowstr::ToCowStr;

    #[test]
    fn parses_did() {
        let ident = AtIdentifier::<&str>::new("did:plc:foo").unwrap();
        assert!(matches!(ident, AtIdentifier::Did(_)));
        assert_eq!(ident.as_str(), "did:plc:foo");
    }

    #[test]
    fn parses_handle() {
        let ident = AtIdentifier::<&str>::new("alice.test").unwrap();
        assert!(matches!(ident, AtIdentifier::Handle(_)));
        assert_eq!(ident.as_str(), "alice.test");
    }

    #[test]
    fn did_takes_precedence() {
        let ident = AtIdentifier::<&str>::new("did:web:alice.test").unwrap();
        assert!(matches!(ident, AtIdentifier::Did(_)));
    }

    #[test]
    fn from_types() {
        let did = Did::<SmolStr>::new_owned("did:plc:foo").unwrap();
        let ident: AtIdentifier<SmolStr> = did.into();
        assert!(matches!(ident, AtIdentifier::Did(_)));

        let handle = Handle::new("alice.test".to_cowstr()).unwrap();
        let ident: AtIdentifier<CowStr> = handle.into();
        assert!(matches!(ident, AtIdentifier::Handle(_)));
    }

    #[test]
    fn owned_construction() {
        let ident = AtIdentifier::<SmolStr>::new_owned("did:plc:foo").unwrap();
        assert!(matches!(ident, AtIdentifier::Did(_)));

        let ident = AtIdentifier::<SmolStr>::new_owned("alice.test").unwrap();
        assert!(matches!(ident, AtIdentifier::Handle(_)));
    }
}
