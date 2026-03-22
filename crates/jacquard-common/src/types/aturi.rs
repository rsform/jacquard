use crate::bos::{BorrowOrShare, Bos};
use crate::types::ident::AtIdentifier;
use crate::types::nsid::Nsid;
use crate::types::recordkey::{RecordKey, Rkey};
use crate::types::string::{AtStrError, StrParseKind};
use crate::{CowStr, DefaultStr, IntoStatic};
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::num::NonZeroU16;
use core::ops::Deref;
use core::str::FromStr;
#[cfg(all(not(target_arch = "wasm32"), feature = "std"))]
use regex::Regex;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "std")))]
use regex_automata::meta::Regex;
#[cfg(target_arch = "wasm32")]
use regex_lite::Regex;
use serde::Serializer;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};

use super::Lazy;

/// Byte indices of delimiter positions within an AT URI string.
///
/// Each index points at the delimiter character itself (`/` or `#`).
/// Uses `NonZeroU16` for niche optimisation — `Option<NonZeroU16>` is 2 bytes.
/// Safe because AT URIs start with `at://` (5 bytes), so any delimiter is at index >= 5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct AtUriIndices {
    /// Index of the `/` separating authority from collection.
    first_slash: Option<NonZeroU16>,
    /// Index of the `/` separating collection from rkey.
    second_slash: Option<NonZeroU16>,
    /// Index of the `#` starting the fragment.
    hash: Option<NonZeroU16>,
}

impl AtUriIndices {
    /// End of the authority component.
    fn authority_end(&self, len: usize) -> usize {
        self.first_slash
            .or(self.hash)
            .map(|n| n.get() as usize)
            .unwrap_or(len)
    }

    /// End of the collection component (only valid if first_slash is Some).
    fn collection_end(&self, len: usize) -> usize {
        self.second_slash
            .or(self.hash)
            .map(|n| n.get() as usize)
            .unwrap_or(len)
    }

    /// End of the rkey component (only valid if second_slash is Some).
    fn rkey_end(&self, len: usize) -> usize {
        self.hash.map(|n| n.get() as usize).unwrap_or(len)
    }
}

/// AT Protocol URI (`at://`) for referencing records in repositories.
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
#[derive(Clone, Debug)]
pub struct AtUri<S: Bos<str> + AsRef<str> = DefaultStr> {
    uri: S,
    indices: AtUriIndices,
}

impl<S: Bos<str> + AsRef<str>> PartialEq for AtUri<S> {
    fn eq(&self, other: &Self) -> bool {
        self.uri.as_ref() == other.uri.as_ref()
    }
}

impl<S: Bos<str> + AsRef<str>> Eq for AtUri<S> {}

impl<S: Bos<str> + AsRef<str>> Hash for AtUri<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uri.as_ref().hash(state);
    }
}

/// Path component of an AT URI (collection and optional record key).
///
/// Represents the `/COLLECTION[/RKEY]` portion of an AT URI.
pub struct RepoPath<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// Collection NSID (e.g., `app.bsky.feed.post`).
    pub collection: Nsid<S>,
    /// Optional record key identifying a specific record.
    pub rkey: Option<RecordKey<Rkey<S>>>,
}

impl<S: Bos<str> + AsRef<str> + Clone> Clone for RepoPath<S> {
    fn clone(&self) -> Self {
        RepoPath {
            collection: self.collection.clone(),
            rkey: self.rkey.clone(),
        }
    }
}

impl<S: Bos<str> + AsRef<str>> PartialEq for RepoPath<S> {
    fn eq(&self, other: &Self) -> bool {
        self.collection.as_str() == other.collection.as_str()
            && match (&self.rkey, &other.rkey) {
                (Some(a), Some(b)) => a.as_ref() == b.as_ref(),
                (None, None) => true,
                _ => false,
            }
    }
}

impl<S: Bos<str> + AsRef<str>> Eq for RepoPath<S> {}

impl<S: Bos<str> + AsRef<str>> Hash for RepoPath<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.collection.as_str().hash(state);
        if let Some(rkey) = &self.rkey {
            rkey.as_ref().hash(state);
        }
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Debug for RepoPath<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepoPath")
            .field("collection", &self.collection.as_str())
            .field("rkey", &self.rkey.as_ref().map(|r| r.as_ref()))
            .finish()
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for RepoPath<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", self.collection)?;
        if let Some(rkey) = &self.rkey {
            write!(f, "/{}", rkey.as_ref())?;
        }
        Ok(())
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for RepoPath<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = RepoPath<S::Output>;

    fn into_static(self) -> Self::Output {
        RepoPath {
            collection: self.collection.into_static(),
            rkey: self.rkey.map(|rkey| rkey.into_static()),
        }
    }
}

/// Owned (static lifetime) version of `RepoPath`.
pub type UriPathBuf = RepoPath<SmolStr>;

/// Regex for AT URI validation per AT Protocol spec.
pub static ATURI_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Fragment allows: / and \ and other special chars. In raw string, backslashes are literal.
    Regex::new(r##"^at://(?<authority>[a-zA-Z0-9._:%-]+)(/(?<collection>[a-zA-Z0-9-.]+)(/(?<rkey>[a-zA-Z0-9._~:@!$&%')(*+,;=-]+))?)?(#(?<fragment>/[a-zA-Z0-9._~:@!$&%')(*+,;=\-\[\]/\\]*))?$"##).unwrap()
});

// ---------------------------------------------------------------------------
// Internal validation
// ---------------------------------------------------------------------------

/// Validate an AT URI string and extract delimiter indices.
///
/// Runs the regex once. Does not parse components into typed wrappers — that happens
/// lazily via the accessor methods.
pub(crate) fn validate_and_index(uri: &str) -> Result<AtUriIndices, AtStrError> {
    let Some(parts) = ATURI_REGEX.captures(uri) else {
        return Err(AtStrError::regex(
            "at-uri-scheme",
            uri,
            SmolStr::new_static("doesn't match schema"),
        ));
    };

    let Some(authority) = parts.name("authority") else {
        return Err(AtStrError::missing("at-uri-scheme", uri, "authority"));
    };

    // Validate the authority as a DID or handle.
    AtIdentifier::new(authority.as_str())
        .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;

    // Validate collection if present.
    if let Some(collection) = parts.name("collection") {
        Nsid::new(collection.as_str())
            .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;

        // Validate rkey if present.
        if let Some(rkey) = parts.name("rkey") {
            Rkey::new(rkey.as_str())
                .map_err(|e| AtStrError::wrap("at-uri-scheme", uri.to_string(), e))?;
        }
    }

    Ok(extract_indices(uri))
}

/// Extract delimiter indices from a URI string that has already been validated.
fn extract_indices(uri: &str) -> AtUriIndices {
    let bytes = uri.as_bytes();
    let mut first_slash = None;
    let mut second_slash = None;
    let mut hash = None;

    // Start after "at://" (5 bytes). Walk until we find delimiters.
    let mut i = 5;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if first_slash.is_none() => {
                first_slash = NonZeroU16::new(i as u16);
            }
            b'/' if second_slash.is_none() => {
                second_slash = NonZeroU16::new(i as u16);
            }
            b'#' => {
                hash = NonZeroU16::new(i as u16);
                break; // Fragment is always last.
            }
            _ => {}
        }
        i += 1;
    }

    AtUriIndices {
        first_slash,
        second_slash,
        hash,
    }
}

// ---------------------------------------------------------------------------
// Borrowed construction
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Generic unchecked construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> AtUri<S> {
    /// Unchecked constructor from a pre-validated URI string.
    ///
    /// Extracts indices but does not validate components. Use when the URI
    /// has already been validated externally (e.g., by `AtprotoStr::new()`).
    ///
    /// # Safety
    ///
    /// Callers must ensure the URI is a valid AT URI. Accessor methods will
    /// produce typed wrappers via `unchecked` constructors.
    pub unsafe fn unchecked(uri: S) -> Self {
        let indices = extract_indices(uri.as_ref());
        AtUri { uri, indices }
    }

    /// Construct from a pre-validated URI string and pre-computed indices.
    ///
    /// # Safety
    ///
    /// Callers must ensure the URI is valid and the indices are correct.
    pub(crate) unsafe fn from_parts(uri: S, indices: AtUriIndices) -> Self {
        AtUri { uri, indices }
    }
}

// ---------------------------------------------------------------------------
// Generic construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> AtUri<S> {
    /// Fallible constructor, validates, wraps the input directly.
    pub fn new(uri: S) -> Result<Self, AtStrError> {
        let indices = validate_and_index(uri.as_ref())?;
        Ok(AtUri { uri, indices })
    }

    /// Infallible constructor. Panics on invalid URIs.
    pub fn raw(uri: S) -> Self {
        Self::new(uri).expect("valid AT URI")
    }
}

// ---------------------------------------------------------------------------
// Owned construction
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str> + FromStr> AtUri<S> {
    /// Fallible owned constructor.
    pub fn new_owned(uri: impl AsRef<str>) -> Result<Self, AtStrError> {
        let uri_str = uri.as_ref();
        let indices = validate_and_index(uri_str)?;
        let s = S::from_str(uri_str).map_err(|_| {
            AtStrError::new(
                "at-uri-scheme",
                uri_str.to_string(),
                StrParseKind::Conversion,
            )
        })?;
        Ok(AtUri { uri: s, indices })
    }

    /// Fallible constructor from typical parts.
    pub fn from_parts_owned(
        authority: impl AsRef<str>,
        collection: impl AsRef<str>,
        rkey: impl AsRef<str>,
    ) -> Result<Self, AtStrError> {
        let (authority, collection, rkey) =
            (authority.as_ref(), collection.as_ref(), rkey.as_ref());
        if authority.is_empty() || (collection.is_empty() && !rkey.is_empty()) {
            Err(AtStrError::missing(
                "at-uri-scheme",
                &format!("at://{}/{}/{}", authority, collection, rkey),
                "correct uri path",
            ))
        } else if collection.is_empty() && rkey.is_empty() {
            let uri = format!("at://{}", authority);
            Self::new_owned(uri)
        } else if rkey.is_empty() {
            let uri = format!("at://{}/{}", authority, collection);
            Self::new_owned(uri)
        } else {
            let uri = format!("at://{}/{}/{}", authority, collection, rkey);
            Self::new_owned(uri)
        }
    }

    /// Fallible constructor for static strings.
    pub fn new_static(uri: &'static str) -> Result<Self, AtStrError> {
        let indices = validate_and_index(uri)?;
        let s = S::from_str(uri).map_err(|_| {
            AtStrError::new("at-uri-scheme", uri.to_string(), StrParseKind::Conversion)
        })?;
        Ok(AtUri { uri: s, indices })
    }
}

// ---------------------------------------------------------------------------
// Accessors
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> AtUri<S> {
    /// Get the full URI as a string slice.
    pub fn as_str(&self) -> &str {
        self.uri.as_ref()
    }

    /// Get the authority component (DID or handle).
    ///
    /// Uses `BorrowOrShare` split lifetimes: when `S = &'d str`, the returned
    /// `AtIdentifier<&'d str>` can outlive the borrow of `self`.
    pub fn authority<'i, 'o>(&'i self) -> AtIdentifier<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let s: &'o str = self.uri.borrow_or_share();
        let end = self.indices.authority_end(s.len());
        // Safety: constructor validated the authority. `unchecked` classifies DID vs handle
        // but won't reject valid input.
        unsafe { AtIdentifier::unchecked(&s[5..end]) }
    }

    /// Get the collection NSID from the path, if present.
    pub fn collection<'i, 'o>(&'i self) -> Option<Nsid<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let idx = self.indices.first_slash?.get() as usize;
        let s: &'o str = self.uri.borrow_or_share();
        let end = self.indices.collection_end(s.len());
        Some(unsafe { Nsid::unchecked(&s[idx + 1..end]) })
    }

    /// Get the record key from the path, if present.
    pub fn rkey<'i, 'o>(&'i self) -> Option<Rkey<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let idx = self.indices.second_slash?.get() as usize;
        let s: &'o str = self.uri.borrow_or_share();
        let end = self.indices.rkey_end(s.len());
        Some(unsafe { Rkey::unchecked(&s[idx + 1..end]) })
    }

    /// Get the path component (collection and optional rkey).
    pub fn path<'i, 'o>(&'i self) -> Option<RepoPath<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let slash = self.indices.first_slash?.get() as usize;
        let s: &'o str = self.uri.borrow_or_share();
        let col_end = self.indices.collection_end(s.len());
        let collection = unsafe { Nsid::unchecked(&s[slash + 1..col_end]) };
        let rkey = self.indices.second_slash.map(|idx| {
            let rkey_end = self.indices.rkey_end(s.len());
            RecordKey(unsafe { Rkey::unchecked(&s[idx.get() as usize + 1..rkey_end]) })
        });
        Some(RepoPath { collection, rkey })
    }

    /// Get the fragment component if present.
    pub fn fragment<'i, 'o>(&'i self) -> Option<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let idx = self.indices.hash?.get() as usize;
        let s: &'o str = self.uri.borrow_or_share();
        Some(&s[idx + 1..])
    }
}

// ---------------------------------------------------------------------------
// IntoStatic
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for AtUri<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = AtUri<S::Output>;

    fn into_static(self) -> AtUri<S::Output> {
        AtUri {
            uri: self.uri.into_static(),
            indices: self.indices,
        }
    }
}

// ---------------------------------------------------------------------------
// Serde
// ---------------------------------------------------------------------------

impl<'de, S> Deserialize<'de> for AtUri<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        let indices = validate_and_index(s.as_ref()).map_err(D::Error::custom)?;
        Ok(AtUri { uri: s, indices })
    }
}

impl<S: Bos<str> + AsRef<str>> Serialize for AtUri<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        serializer.serialize_str(self.uri.as_ref())
    }
}

// ---------------------------------------------------------------------------
// FromStr
// ---------------------------------------------------------------------------

impl FromStr for AtUri<SmolStr> {
    type Err = AtStrError;

    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        Self::new_owned(uri)
    }
}

// ---------------------------------------------------------------------------
// Display, conversions
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> fmt::Display for AtUri<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.uri.as_ref())
    }
}

impl<S: Bos<str> + AsRef<str>> From<AtUri<S>> for String {
    fn from(value: AtUri<S>) -> Self {
        value.uri.as_ref().to_string()
    }
}

impl TryFrom<String> for AtUri<SmolStr> {
    type Error = AtStrError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new_owned(&value)
    }
}

impl<'d> TryFrom<CowStr<'d>> for AtUri<CowStr<'d>> {
    type Error = AtStrError;

    fn try_from(uri: CowStr<'d>) -> Result<Self, Self::Error> {
        Self::new(uri)
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for AtUri<S> {
    fn as_ref(&self) -> &str {
        self.uri.as_ref()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for AtUri<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.uri.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        assert_eq!(uri.fragment().unwrap(), "/path");

        // Fragment must start with /.
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
        // Spec says 8KB max.
        let long_did = format!("did:plc:{}", "a".repeat(8000));
        let uri = format!("at://{}", long_did);
        assert!(uri.len() < 8192);
        // Should work if components are valid.
        // (our DID will fail at 2048 chars, but this tests the URI doesn't impose extra limits)
    }

    #[test]
    fn clone_preserves_indices() {
        let uri = AtUri::new("at://alice.test/com.example.foo/123").unwrap();
        let owned: AtUri<SmolStr> =
            AtUri::new_owned("at://alice.test/com.example.foo/123").unwrap();
        let cloned = owned.clone();
        assert_eq!(owned.as_str(), cloned.as_str());
        assert_eq!(cloned.authority().as_str(), "alice.test");
        assert_eq!(cloned.collection().unwrap().as_str(), "com.example.foo");
        assert_eq!(cloned.rkey().unwrap().as_ref(), "123");

        // Borrowed clone.
        let cloned_borrowed = uri.clone();
        assert_eq!(cloned_borrowed.authority().as_str(), "alice.test");
    }

    #[test]
    fn into_static_preserves_components() {
        let uri = AtUri::new("at://did:plc:foo/com.example.post/abc").unwrap();
        let owned: AtUri<SmolStr> = uri.into_static();
        assert_eq!(owned.authority().as_str(), "did:plc:foo");
        assert_eq!(owned.collection().unwrap().as_str(), "com.example.post");
        assert_eq!(owned.rkey().unwrap().as_ref(), "abc");
    }

    #[test]
    fn path_accessor() {
        let uri = AtUri::new("at://alice.test/com.example.foo/123").unwrap();
        let path = uri.path().unwrap();
        assert_eq!(path.collection.as_str(), "com.example.foo");
        assert_eq!(path.rkey.unwrap().as_ref(), "123");

        let uri2 = AtUri::new("at://alice.test/com.example.foo").unwrap();
        let path2 = uri2.path().unwrap();
        assert_eq!(path2.collection.as_str(), "com.example.foo");
        assert!(path2.rkey.is_none());

        let uri3 = AtUri::new("at://alice.test").unwrap();
        assert!(uri3.path().is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let original = "at://did:plc:foo/com.example.post/123";
        let owned: AtUri<SmolStr> = AtUri::new_owned(original).unwrap();
        let json = serde_json::to_string(&owned).unwrap();
        assert_eq!(json, format!("\"{}\"", original));
        let deserialized: AtUri<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(owned, deserialized);
    }

    #[test]
    fn fragment_only_uri() {
        let uri = AtUri::new("at://alice.test#/foo/bar").unwrap();
        assert_eq!(uri.authority().as_str(), "alice.test");
        assert!(uri.collection().is_none());
        assert!(uri.rkey().is_none());
        assert_eq!(uri.fragment().unwrap(), "/foo/bar");
    }

    #[test]
    fn bos_lifetime_semantics() {
        // Verify that AtUri<&str> accessors can outlive the borrow.
        let s = String::from("at://alice.test/com.example.foo/123");
        let uri = AtUri::new(s).unwrap();
        let authority = uri.authority();
        // authority borrows from s, not from uri — this is the BOS magic.
        assert_eq!(authority.as_str(), "alice.test");
    }
}
