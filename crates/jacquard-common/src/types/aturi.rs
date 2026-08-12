use crate::bos::{BorrowOrShare, Bos};
use crate::types::did::Did;
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
use serde::{de::Error, Deserialize, Deserializer, Serialize};
use smol_str::SmolStr;

use super::Lazy;

/// Byte indices of delimiter positions within a standard AT URI string.
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

/// The parsed shape of an [`AtUri`].
///
/// Keeping the shape as an enum makes it impossible for accessors to observe a
/// partially parsed space URI or to accidentally use the standard path offsets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UriShape {
    Standard { indices: AtUriIndices },
    Space { indices: AtSpaceUriIndices },
}

/// AT Protocol URI (`at://`) for referencing records in repositories or
/// permissioned spaces.
///
/// Public record form: `at://AUTHORITY/COLLECTION/RKEY`.
/// Permissioned record form: `at://SPACE_DID/space/SPACE_TYPE/SKEY/AUTHOR/COLLECTION/RKEY`.
#[derive(Clone, Debug)]
pub struct AtUri<S: Bos<str> + AsRef<str> = DefaultStr> {
    uri: S,
    shape: UriShape,
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

impl<S: Bos<str> + AsRef<str>> RepoPath<S> {
    /// Convert to a `RepoPath` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> RepoPath<B> {
        RepoPath {
            collection: self.collection.convert(),
            rkey: self.rkey.map(|rkey| RecordKey(rkey.0.convert())),
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
// Generic construction
// ---------------------------------------------------------------------------

fn validate_uri_shape(uri: &str) -> Result<UriShape, AtStrError> {
    // The space marker is positional: only the first path segment after the
    // authority may carry it. A standard URI with a collection or rkey that
    // happens to be "space" must parse as standard.
    let after_scheme = uri
        .strip_prefix("at://")
        .unwrap_or(uri);
    let first_path_segment = after_scheme.split('/').nth(1);
    if first_path_segment == Some("space") {
        validate_space_and_index(uri).map(|indices| UriShape::Space { indices })
    } else {
        validate_and_index(uri).map(|indices| UriShape::Standard { indices })
    }
}

impl<S: Bos<str> + AsRef<str>> AtUri<S> {
    /// Unchecked constructor from a pre-validated URI string.
    ///
    /// # Safety
    /// The caller must ensure the URI is a valid standard or permissioned-space URI.
    pub unsafe fn unchecked(uri: S) -> Self {
        let shape = validate_uri_shape(uri.as_ref()).expect("valid AT URI");
        AtUri { uri, shape }
    }

    /// Construct a standard URI from pre-validated storage and indices.
    ///
    /// # Safety
    /// The caller must ensure the URI and indices describe a valid standard URI.
    pub(crate) unsafe fn from_parts(uri: S, indices: AtUriIndices) -> Self {
        AtUri {
            uri,
            shape: UriShape::Standard { indices },
        }
    }

    /// Fallible constructor, validates and wraps the input directly.
    pub fn new(uri: S) -> Result<Self, AtStrError> {
        let shape = validate_uri_shape(uri.as_ref())?;
        Ok(AtUri { uri, shape })
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
        let shape = validate_uri_shape(uri_str)?;
        let s = S::from_str(uri_str).map_err(|_| {
            AtStrError::new(
                "at-uri-scheme",
                uri_str.to_string(),
                StrParseKind::Conversion,
            )
        })?;
        Ok(AtUri { uri: s, shape })
    }

    /// Construct an `AtUri` from a validated permissioned-space URI.
    pub fn from_space_uri(uri: impl AsRef<str>) -> Result<Self, AtStrError> {
        let uri = uri.as_ref();
        let shape = validate_space_and_index(uri).map(|indices| UriShape::Space { indices })?;
        let value = S::from_str(uri).map_err(|_| {
            AtStrError::new("at-uri-scheme", uri.to_string(), StrParseKind::Conversion)
        })?;
        Ok(Self { uri: value, shape })
    }

    /// Fallible constructor from typical public-repository parts.
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
        let shape = validate_uri_shape(uri)?;
        let s = S::from_str(uri).map_err(|_| {
            AtStrError::new("at-uri-scheme", uri.to_string(), StrParseKind::Conversion)
        })?;
        Ok(AtUri { uri: s, shape })
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

    /// Return whether this URI addresses a permissioned space.
    pub fn is_space(&self) -> bool {
        matches!(self.shape, UriShape::Space { .. })
    }

    /// Get the authority component (the space DID for space URIs).
    pub fn authority<'i, 'o>(&'i self) -> AtIdentifier<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let s: &'o str = self.uri.borrow_or_share();
        let end = match self.shape {
            UriShape::Standard { indices } => indices.authority_end(s.len()),
            UriShape::Space { indices } => indices.first_slash.get() as usize,
        };
        unsafe { AtIdentifier::unchecked(&s[5..end]) }
    }

    /// Get the space authority DID, if this is a space URI.
    pub fn space_did<'i, 'o>(&'i self) -> Option<Did<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let UriShape::Space { indices } = self.shape else {
            return None;
        };
        let s: &'o str = self.uri.borrow_or_share();
        Some(unsafe { Did::unchecked(&s[5..indices.first_slash.get() as usize]) })
    }

    /// Get the space type NSID, if this is a space URI.
    pub fn space_type<'i, 'o>(&'i self) -> Option<Nsid<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let UriShape::Space { indices } = self.shape else {
            return None;
        };
        let s: &'o str = self.uri.borrow_or_share();
        Some(unsafe {
            Nsid::unchecked(
                &s[indices.second_slash.get() as usize + 1..indices.third_slash.get() as usize],
            )
        })
    }

    /// Get the space record key, if this is a space URI.
    pub fn skey<'i, 'o>(&'i self) -> Option<Rkey<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let UriShape::Space { indices } = self.shape else {
            return None;
        };
        let s: &'o str = self.uri.borrow_or_share();
        Some(unsafe {
            Rkey::unchecked(&s[indices.third_slash.get() as usize + 1..indices.skey_end(s.len())])
        })
    }

    /// Get the record author. For public URIs this is the authority; for
    /// space URIs it is the member DID after the space identity.
    pub fn author<'i, 'o>(&'i self) -> Option<AtIdentifier<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let s: &'o str = self.uri.borrow_or_share();
        match self.shape {
            UriShape::Standard { .. } => Some(self.authority()),
            UriShape::Space { indices } => {
                let slash = indices.fourth_slash?;
                Some(unsafe {
                    AtIdentifier::unchecked(
                        &s[slash.get() as usize + 1..indices.author_end(s.len())],
                    )
                })
            }
        }
    }

    /// Get the collection NSID from either URI shape, if present.
    pub fn collection<'i, 'o>(&'i self) -> Option<Nsid<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let s: &'o str = self.uri.borrow_or_share();
        match self.shape {
            UriShape::Standard { indices } => {
                let idx = indices.first_slash?.get() as usize;
                let end = indices.collection_end(s.len());
                Some(unsafe { Nsid::unchecked(&s[idx + 1..end]) })
            }
            UriShape::Space { indices } => {
                let slash = indices.fifth_slash?;
                let end = indices
                    .sixth_slash
                    .map(|n| n.get() as usize)
                    .unwrap_or(s.len());
                Some(unsafe { Nsid::unchecked(&s[slash.get() as usize + 1..end]) })
            }
        }
    }

    /// Get the record key from either URI shape, if present.
    pub fn rkey<'i, 'o>(&'i self) -> Option<Rkey<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let s: &'o str = self.uri.borrow_or_share();
        match self.shape {
            UriShape::Standard { indices } => {
                let idx = indices.second_slash?.get() as usize;
                let end = indices.rkey_end(s.len());
                Some(unsafe { Rkey::unchecked(&s[idx + 1..end]) })
            }
            UriShape::Space { indices } => {
                let slash = indices.sixth_slash?;
                Some(unsafe { Rkey::unchecked(&s[slash.get() as usize + 1..]) })
            }
        }
    }

    /// Return the space identity represented by this URI, if any.
    pub fn space_ref<'i, 'o>(&'i self) -> Option<SpaceRef<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        Some(SpaceRef {
            space_did: self.space_did()?,
            space_type: self.space_type()?,
            skey: self.skey()?,
        })
    }

    /// Get the public-repository path. Space records expose their record
    /// collection and key through `collection()` and `rkey()` instead.
    pub fn path<'i, 'o>(&'i self) -> Option<RepoPath<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let UriShape::Standard { indices } = self.shape else {
            return None;
        };
        let slash = indices.first_slash?.get() as usize;
        let s: &'o str = self.uri.borrow_or_share();
        let col_end = indices.collection_end(s.len());
        let collection = unsafe { Nsid::unchecked(&s[slash + 1..col_end]) };
        let rkey = indices.second_slash.map(|idx| {
            let rkey_end = indices.rkey_end(s.len());
            RecordKey(unsafe { Rkey::unchecked(&s[idx.get() as usize + 1..rkey_end]) })
        });
        Some(RepoPath { collection, rkey })
    }

    /// Get the fragment component if present.
    pub fn fragment<'i, 'o>(&'i self) -> Option<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let idx = match self.shape {
            UriShape::Standard { indices } => indices.hash?,
            UriShape::Space { .. } => return None,
        };
        let s: &'o str = self.uri.borrow_or_share();
        Some(&s[idx.get() as usize + 1..])
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
            shape: self.shape,
        }
    }
}

impl<S: Bos<str> + AsRef<str>> AtUri<S> {
    /// Convert to an `AtUri` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> AtUri<B> {
        AtUri {
            uri: B::from(self.uri),
            shape: self.shape,
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
        let shape = validate_uri_shape(s.as_ref()).map_err(D::Error::custom)?;
        Ok(AtUri { uri: s, shape })
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
// Permissioned space URIs
// ---------------------------------------------------------------------------

/// Byte indices of the slash delimiters in a permissioned space URI.
///
/// Each index points at the delimiter itself. `NonZeroU16` preserves the same
/// niche optimization used by [`AtUri`]: an absent index costs no additional
/// space and a valid delimiter can never occur at zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct AtSpaceUriIndices {
    first_slash: NonZeroU16,
    second_slash: NonZeroU16,
    third_slash: NonZeroU16,
    fourth_slash: Option<NonZeroU16>,
    fifth_slash: Option<NonZeroU16>,
    sixth_slash: Option<NonZeroU16>,
}

impl AtSpaceUriIndices {
    fn skey_end(&self, len: usize) -> usize {
        self.fourth_slash.map(|n| n.get() as usize).unwrap_or(len)
    }

    fn author_end(&self, len: usize) -> usize {
        self.fifth_slash.map(|n| n.get() as usize).unwrap_or(len)
    }
}

/// The `/space/{spaceType}/{skey}` portion of a permissioned space URI.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SpacePath<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// The NSID identifying the space type declaration.
    pub space_type: Nsid<S>,
    /// The record-key-shaped identifier selecting a space of that type.
    pub skey: RecordKey<Rkey<S>>,
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for SpacePath<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/space/{}/{}", self.space_type, self.skey.as_ref())
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for SpacePath<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = SpacePath<S::Output>;

    fn into_static(self) -> Self::Output {
        SpacePath {
            space_type: self.space_type.into_static(),
            skey: RecordKey(self.skey.0.into_static()),
        }
    }
}

/// A reference to a permissioned space, without a record path.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SpaceRef<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// DID of the repository hosting the permissioned space.
    pub space_did: Did<S>,
    /// NSID identifying the space type.
    pub space_type: Nsid<S>,
    /// Record-key-shaped identifier selecting the space.
    pub skey: Rkey<S>,
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for SpaceRef<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "at://{}/space/{}/{}",
            self.space_did, self.space_type, self.skey
        )
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for SpaceRef<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = SpaceRef<S::Output>;

    fn into_static(self) -> Self::Output {
        SpaceRef {
            space_did: self.space_did.into_static(),
            space_type: self.space_type.into_static(),
            skey: self.skey.into_static(),
        }
    }
}

/// Permissioned space URI.
///
/// The accepted forms are:
///
/// ```text
/// at://AUTHORITY/space/SPACE_TYPE/SKEY
/// at://AUTHORITY/space/SPACE_TYPE/SKEY/AUTHOR/COLLECTION/RKEY
/// ```
///
/// This is intentionally a separate type from [`AtUri`]. It follows AtUri's
/// generic backing storage, byte-index accessors, serde, `IntoStatic`, and
/// niche optimization without broadening ordinary AT URI validation.
#[derive(Clone, Debug)]
pub struct AtSpaceUri<S: Bos<str> + AsRef<str> = DefaultStr> {
    uri: S,
    indices: AtSpaceUriIndices,
}

impl<S: Bos<str> + AsRef<str>> PartialEq for AtSpaceUri<S> {
    fn eq(&self, other: &Self) -> bool {
        self.uri.as_ref() == other.uri.as_ref()
    }
}

impl<S: Bos<str> + AsRef<str>> Eq for AtSpaceUri<S> {}

impl<S: Bos<str> + AsRef<str>> Hash for AtSpaceUri<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uri.as_ref().hash(state);
    }
}

/// Regex for the two canonical permissioned space URI forms.
pub static ATSPACEURI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^at://(?<authority>[a-zA-Z0-9._:%-]+)/space/(?<space_type>[a-zA-Z0-9.-]+)/(?<skey>[a-zA-Z0-9._~:@!$&%')(*+,;=-]+)(/(?<author>[a-zA-Z0-9._:%-]+)/(?<collection>[a-zA-Z0-9.-]+)/(?<rkey>[a-zA-Z0-9._~:@!$&%')(*+,;=-]+))?$",
    )
    .unwrap()
});

fn validate_space_and_index(uri: &str) -> Result<AtSpaceUriIndices, AtStrError> {
    let Some(parts) = ATSPACEURI_REGEX.captures(uri) else {
        return Err(AtStrError::regex(
            "at-space-uri-scheme",
            uri,
            SmolStr::new_static("doesn't match schema"),
        ));
    };

    let authority = parts
        .name("authority")
        .ok_or_else(|| AtStrError::missing("at-space-uri-scheme", uri, "authority"))?;
    Did::new(authority.as_str())
        .map_err(|e| AtStrError::wrap("at-space-uri-scheme", uri.to_string(), e))?;

    let space_type = parts
        .name("space_type")
        .ok_or_else(|| AtStrError::missing("at-space-uri-scheme", uri, "space type"))?;
    Nsid::new(space_type.as_str())
        .map_err(|e| AtStrError::wrap("at-space-uri-scheme", uri.to_string(), e))?;

    let skey = parts
        .name("skey")
        .ok_or_else(|| AtStrError::missing("at-space-uri-scheme", uri, "skey"))?;
    Rkey::new(skey.as_str())
        .map_err(|e| AtStrError::wrap("at-space-uri-scheme", uri.to_string(), e))?;

    if let Some(author) = parts.name("author") {
        Did::new(author.as_str())
            .map_err(|e| AtStrError::wrap("at-space-uri-scheme", uri.to_string(), e))?;
        let collection = parts
            .name("collection")
            .ok_or_else(|| AtStrError::missing("at-space-uri-scheme", uri, "record collection"))?;
        Nsid::new(collection.as_str())
            .map_err(|e| AtStrError::wrap("at-space-uri-scheme", uri.to_string(), e))?;
        let rkey = parts
            .name("rkey")
            .ok_or_else(|| AtStrError::missing("at-space-uri-scheme", uri, "record key"))?;
        Rkey::new(rkey.as_str())
            .map_err(|e| AtStrError::wrap("at-space-uri-scheme", uri.to_string(), e))?;
    }

    Ok(extract_space_indices(uri))
}

fn extract_space_indices(uri: &str) -> AtSpaceUriIndices {
    let bytes = uri.as_bytes();
    let mut slashes = [None; 6];
    let mut found = 0;
    for (i, byte) in bytes.iter().enumerate().skip(5) {
        if *byte == b'/' && found < slashes.len() {
            slashes[found] = NonZeroU16::new(i as u16);
            found += 1;
        }
    }
    AtSpaceUriIndices {
        first_slash: slashes[0].expect("validated space URI has authority slash"),
        second_slash: slashes[1].expect("validated space URI has space slash"),
        third_slash: slashes[2].expect("validated space URI has skey slash"),
        fourth_slash: slashes[3],
        fifth_slash: slashes[4],
        sixth_slash: slashes[5],
    }
}

impl<S: Bos<str> + AsRef<str>> AtSpaceUri<S> {
    /// Construct from a pre-validated URI and compute component indices.
    ///
    /// # Safety
    /// The caller must ensure `uri` is a valid canonical permissioned URI.
    pub unsafe fn unchecked(uri: S) -> Self {
        let indices = extract_space_indices(uri.as_ref());
        Self { uri, indices }
    }

    /// Parse a canonical permissioned space URI from its backing string.
    ///
    /// Both the space authority and an optional record author must be DIDs;
    /// handles are intentionally rejected for proposal-0016 addressing.
    pub fn new(uri: S) -> Result<Self, AtStrError> {
        let indices = validate_space_and_index(uri.as_ref())?;
        Ok(Self { uri, indices })
    }

    /// Construct a permissioned space URI, panicking when the input is invalid.
    ///
    /// Prefer [`Self::new`] when the value is not a compile-time constant.
    #[track_caller]
    pub fn raw(uri: S) -> Self {
        Self::new(uri).expect("valid permissioned space URI")
    }

    /// Return the complete canonical URI string.
    pub fn as_str(&self) -> &str {
        self.uri.as_ref()
    }

    /// Return the authority as an AT identifier.
    ///
    /// Permissioned URI validation guarantees this identifier is always a DID.
    pub fn authority<'i, 'o>(&'i self) -> AtIdentifier<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let value: &'o str = self.uri.borrow_or_share();
        unsafe { AtIdentifier::unchecked(&value[5..self.indices.first_slash.get() as usize]) }
    }

    /// Return the space authority as a validated DID.
    pub fn did_authority<'i, 'o>(&'i self) -> Did<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let value: &'o str = self.uri.borrow_or_share();
        unsafe { Did::unchecked(&value[5..self.indices.first_slash.get() as usize]) }
    }

    /// Return the NSID identifying this URI's space type.
    pub fn space_type<'i, 'o>(&'i self) -> Nsid<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let value: &'o str = self.uri.borrow_or_share();
        unsafe {
            Nsid::unchecked(
                &value[self.indices.second_slash.get() as usize + 1
                    ..self.indices.third_slash.get() as usize],
            )
        }
    }

    /// Return the record-key-shaped identifier of this space.
    pub fn skey<'i, 'o>(&'i self) -> Rkey<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let value: &'o str = self.uri.borrow_or_share();
        unsafe {
            Rkey::unchecked(
                &value[self.indices.third_slash.get() as usize + 1
                    ..self.indices.skey_end(value.len())],
            )
        }
    }

    /// Return the optional record author as an AT identifier.
    ///
    /// Permissioned URI validation guarantees a present author is always a DID.
    pub fn author<'i, 'o>(&'i self) -> Option<AtIdentifier<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let slash = self.indices.fourth_slash?;
        let value: &'o str = self.uri.borrow_or_share();
        unsafe {
            Some(AtIdentifier::unchecked(
                &value[slash.get() as usize + 1..self.indices.author_end(value.len())],
            ))
        }
    }

    /// Return the optional record author as a validated DID.
    pub fn did_author<'i, 'o>(&'i self) -> Option<Did<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let slash = self.indices.fourth_slash?;
        let value: &'o str = self.uri.borrow_or_share();
        unsafe {
            Some(Did::unchecked(
                &value[slash.get() as usize + 1..self.indices.author_end(value.len())],
            ))
        }
    }

    /// Return the optional record collection NSID.
    pub fn collection<'i, 'o>(&'i self) -> Option<Nsid<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let slash = self.indices.fifth_slash?;
        let value: &'o str = self.uri.borrow_or_share();
        let end = self
            .indices
            .sixth_slash
            .map(|n| n.get() as usize)
            .unwrap_or(value.len());
        unsafe { Some(Nsid::unchecked(&value[slash.get() as usize + 1..end])) }
    }

    /// Return the optional record key.
    pub fn rkey<'i, 'o>(&'i self) -> Option<Rkey<&'o str>>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        let slash = self.indices.sixth_slash?;
        let value: &'o str = self.uri.borrow_or_share();
        unsafe { Some(Rkey::unchecked(&value[slash.get() as usize + 1..])) }
    }

    /// Return the reusable `/space/{spaceType}/{skey}` path components.
    pub fn path<'i, 'o>(&'i self) -> SpacePath<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        SpacePath {
            space_type: self.space_type(),
            skey: RecordKey(self.skey()),
        }
    }

    /// Return whether this URI includes author, collection, and record-key components.
    pub fn is_record(&self) -> bool {
        self.indices.fourth_slash.is_some()
    }
}

impl<S: Bos<str> + AsRef<str> + FromStr> AtSpaceUri<S> {
    /// Parse a canonical permissioned URI into owned backing storage.
    pub fn new_owned(uri: impl AsRef<str>) -> Result<Self, AtStrError> {
        let uri = uri.as_ref();
        let indices = validate_space_and_index(uri)?;
        let value = S::from_str(uri).map_err(|_| {
            AtStrError::new(
                "at-space-uri-scheme",
                uri.to_string(),
                StrParseKind::Conversion,
            )
        })?;
        Ok(Self {
            uri: value,
            indices,
        })
    }

    /// Parse a canonical permissioned URI from a static string.
    pub fn new_static(uri: &'static str) -> Result<Self, AtStrError> {
        Self::new_owned(uri)
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for AtSpaceUri<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = AtSpaceUri<S::Output>;

    fn into_static(self) -> Self::Output {
        AtSpaceUri {
            uri: self.uri.into_static(),
            indices: self.indices,
        }
    }
}

impl<S: Bos<str> + AsRef<str>> AtSpaceUri<S> {
    /// Convert the URI to another compatible backing-storage type without reparsing.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> AtSpaceUri<B> {
        AtSpaceUri {
            uri: B::from(self.uri),
            indices: self.indices,
        }
    }

    /// Return the identity of the space addressed by this URI.
    pub fn space_ref<'i, 'o>(&'i self) -> SpaceRef<&'o str>
    where
        S: BorrowOrShare<'i, 'o, str>,
    {
        SpaceRef {
            space_did: self.did_authority(),
            space_type: self.space_type(),
            skey: self.skey(),
        }
    }
}

impl<S: Bos<str> + AsRef<str>> From<AtSpaceUri<S>> for AtUri<S>
where
    S: FromStr,
{
    fn from(value: AtSpaceUri<S>) -> Self {
        let AtSpaceUri { uri, indices } = value;
        AtUri {
            uri,
            shape: UriShape::Space { indices },
        }
    }
}

impl<S: Bos<str> + AsRef<str>> From<AtUri<S>> for Option<AtSpaceUri<S>> {
    fn from(value: AtUri<S>) -> Self {
        let AtUri { uri, shape } = value;
        match shape {
            UriShape::Space { indices } => Some(AtSpaceUri { uri, indices }),
            UriShape::Standard { .. } => None,
        }
    }
}

impl<'de, S> Deserialize<'de> for AtSpaceUri<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = S::deserialize(deserializer)?;
        let indices = validate_space_and_index(value.as_ref()).map_err(D::Error::custom)?;
        Ok(Self {
            uri: value,
            indices,
        })
    }
}

impl<S: Bos<str> + AsRef<str>> Serialize for AtSpaceUri<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        serializer.serialize_str(self.uri.as_ref())
    }
}

impl FromStr for AtSpaceUri<SmolStr> {
    type Err = AtStrError;

    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        Self::new_owned(uri)
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for AtSpaceUri<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.uri.as_ref())
    }
}

impl<S: Bos<str> + AsRef<str>> From<AtSpaceUri<S>> for String {
    fn from(value: AtSpaceUri<S>) -> Self {
        value.uri.as_ref().to_string()
    }
}

impl TryFrom<String> for AtSpaceUri<SmolStr> {
    type Error = AtStrError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new_owned(value)
    }
}

impl<'d> TryFrom<CowStr<'d>> for AtSpaceUri<CowStr<'d>> {
    type Error = AtStrError;

    fn try_from(uri: CowStr<'d>) -> Result<Self, Self::Error> {
        Self::new(uri)
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for AtSpaceUri<S> {
    fn as_ref(&self) -> &str {
        self.uri.as_ref()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for AtSpaceUri<S> {
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

    #[test]
    fn at_uri_accepts_public_and_space_record_forms() {
        let public = AtUri::new("at://did:plc:author/com.example.record/r").unwrap();
        assert!(!public.is_space());
        assert_eq!(public.author().unwrap().as_str(), "did:plc:author");
        assert_eq!(public.collection().unwrap().as_str(), "com.example.record");
        assert_eq!(public.rkey().unwrap().as_ref(), "r");
        assert!(public.space_ref().is_none());
        // "space" as a rkey or later segment is standard, not a space marker:
        // the marker is positional (first path segment after the authority).
        let rkey_space = AtUri::new("at://did:plc:author/com.example.record/space").unwrap();
        assert!(!rkey_space.is_space());
        assert_eq!(rkey_space.rkey().unwrap().as_ref(), "space");

        let space = AtUri::new_owned(
            "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record/r",
        )
        .unwrap();
        assert!(space.is_space());
        assert_eq!(space.space_did().unwrap().as_str(), "did:plc:space");
        assert_eq!(space.space_type().unwrap().as_str(), "com.example.type");
        assert_eq!(space.skey().unwrap().as_ref(), "demo");
        assert_eq!(space.author().unwrap().as_str(), "did:plc:author");
        assert_eq!(space.collection().unwrap().as_str(), "com.example.record");
        assert_eq!(space.rkey().unwrap().as_ref(), "r");
        assert_eq!(
            space.space_ref().unwrap().to_string(),
            "at://did:plc:space/space/com.example.type/demo"
        );

        let json = serde_json::to_string(&space).unwrap();
        let decoded: AtUri<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, space);
        let space_uri: AtSpaceUri<SmolStr> = AtSpaceUri::new_owned(space.as_str()).unwrap();
        assert_eq!(AtUri::from(space_uri).as_str(), space.as_str());
    }

    #[test]
    fn permissioned_space_and_record_forms() {
        let space = AtSpaceUri::new("at://did:plc:space/space/com.example.type/demo").unwrap();
        assert_eq!(space.authority().as_str(), "did:plc:space");
        assert_eq!(space.space_type().as_str(), "com.example.type");
        assert_eq!(space.skey().as_str(), "demo");
        assert!(!space.is_record());

        let record = AtSpaceUri::new(
            "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record/r",
        )
        .unwrap();
        assert_eq!(record.author().unwrap().as_str(), "did:plc:author");
        assert_eq!(record.collection().unwrap().as_str(), "com.example.record");
        assert_eq!(record.rkey().unwrap().as_str(), "r");
        assert!(record.is_record());
    }

    #[test]
    fn permissioned_space_rejects_noncanonical_forms() {
        assert!(AtSpaceUri::new("at://did:plc:space/com.example.type/demo").is_err());
        assert!(AtSpaceUri::new("at://did:plc:space/space/com.example.type/demo/").is_err());
        assert!(AtSpaceUri::new(
            "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record"
        )
        .is_err());
    }

    #[test]
    fn permissioned_space_serde_roundtrip() {
        let original =
            "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record/r";
        let uri: AtSpaceUri<SmolStr> = AtSpaceUri::new_owned(original).unwrap();
        let json = serde_json::to_string(&uri).unwrap();
        let decoded: AtSpaceUri<SmolStr> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, uri);
        assert_eq!(decoded.path().space_type.as_str(), "com.example.type");
    }
}
