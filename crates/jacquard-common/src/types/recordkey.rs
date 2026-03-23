use crate::bos::{Bos, DefaultStr};
use crate::types::Literal;
use crate::types::string::{AtStrError, StrParseKind};
use crate::{CowStr, IntoStatic};
use alloc::string::{String, ToString};
use core::fmt;
use core::marker::PhantomData;
use core::ops::Deref;
use core::str::FromStr;
#[cfg(all(not(target_arch = "wasm32"), feature = "std"))]
use regex::Regex;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "std")))]
use regex_automata::meta::Regex;
#[cfg(target_arch = "wasm32")]
use regex_lite::Regex;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};

use super::Lazy;

/// Trait for typed record key implementations
///
/// Allows different record key types (TID, NSID, literals, generic strings) while
/// maintaining validation guarantees. Implementers must ensure compliance with the
/// AT Protocol [record key specification](https://atproto.com/specs/record-key).
///
/// # Safety
/// Implementations must ensure the string representation matches [`RKEY_REGEX`] and
/// is not "." or "..". Built-in implementations: `Tid`, `Nsid`, `Literal<T>`, `Rkey<'_>`.
pub unsafe trait RecordKeyType {
    /// Get the record key as a string slice.
    fn as_str(&self) -> &str;
}

/// Wrapper for typed record keys
///
/// Provides a generic container for different record key types while preserving their
/// specific validation guarantees through the `RecordKeyType` trait.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Hash, Debug)]
#[serde(transparent)]
#[repr(transparent)]
pub struct RecordKey<T: RecordKeyType>(pub T);

impl<S: Bos<str> + AsRef<str> + FromStr> RecordKey<Rkey<S>> {
    /// Create a new `RecordKey` from a static string slice.
    pub fn any_static(str: &'static str) -> Result<Self, AtStrError> {
        Ok(RecordKey(Rkey::new_static(str)?))
    }

    /// Create a new `RecordKey` from an owned string.
    pub fn any_owned(str: impl AsRef<str>) -> Result<Self, AtStrError> {
        Ok(RecordKey(Rkey::new_owned(str)?))
    }
}

impl<S: Bos<str> + AsRef<str>> RecordKey<Rkey<S>> {
    /// Create a new `RecordKey` wrapping a pre-validated Rkey.
    pub fn any(s: S) -> Result<Self, AtStrError> {
        Ok(RecordKey(Rkey::new(s)?))
    }
}

impl<T> From<T> for RecordKey<Rkey>
where
    T: RecordKeyType,
{
    fn from(value: T) -> Self {
        RecordKey(Rkey::new_owned(value.as_str()).expect("Invalid rkey"))
    }
}

impl FromStr for RecordKey<Rkey> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RecordKey(Rkey::from_str(s)?))
    }
}

impl<T> AsRef<str> for RecordKey<T>
where
    T: RecordKeyType,
{
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl<T> IntoStatic for RecordKey<T>
where
    T: IntoStatic + RecordKeyType,
    T::Output: RecordKeyType,
{
    type Output = RecordKey<T::Output>;

    fn into_static(self) -> Self::Output {
        RecordKey(self.0.into_static())
    }
}

impl<T: RecordKeyType> RecordKey<T> {
    /// Convert the inner key to a different type.
    pub fn convert<U: RecordKeyType + From<T>>(self) -> RecordKey<U> {
        RecordKey(U::from(self.0))
    }
}

impl<S: Bos<str> + AsRef<str>> RecordKey<Rkey<S>> {
    /// Borrow as a `RecordKey<Rkey<&str>>`, analogous to `Uri::borrow()`.
    pub fn borrow(&self) -> RecordKey<Rkey<&str>> {
        RecordKey(self.0.borrow())
    }
}

/// AT Protocol record key (generic "any" type)
///
/// Record keys uniquely identify records within a collection. This is the catch-all
/// type for any valid record key string (1-512 characters of alphanumerics, dots,
/// hyphens, underscores, colons, tildes).
///
/// Common record key types:
/// - TID: timestamp-based (most common)
/// - Literal: fixed keys like "self"
/// - NSID: namespaced identifiers
/// - Any: flexible strings matching the validation rules
///
/// See: <https://atproto.com/specs/record-key>
/// AT Protocol record key (generic "any" type).
///
/// See: <https://atproto.com/specs/record-key>
#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Rkey<S: Bos<str> = DefaultStr>(pub(crate) S);

unsafe impl<S: Bos<str> + AsRef<str>> RecordKeyType for Rkey<S> {
    fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

/// Regex for record key validation per AT Protocol spec.
pub static RKEY_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9.\-_:~]{1,512}$").unwrap());

pub(crate) fn validate_rkey(rkey: &str) -> Result<(), AtStrError> {
    if [".", ".."].contains(&rkey) {
        Err(AtStrError::disallowed("record-key", rkey, &[".", ".."]))
    } else if !RKEY_REGEX.is_match(rkey) {
        Err(AtStrError::regex(
            "record-key",
            rkey,
            SmolStr::new_static("doesn't match 'any' schema"),
        ))
    } else {
        Ok(())
    }
}

impl<S: Bos<str> + AsRef<str>> Rkey<S> {
    /// Get the record key as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl<S: Bos<str>> Rkey<S> {
    /// # Safety
    ///
    /// The caller must ensure the rkey is valid.
    pub unsafe fn unchecked(rkey: S) -> Self {
        Rkey(rkey)
    }

    /// Borrow as an `Rkey<&str>`, analogous to `Uri::borrow()`.
    pub fn borrow(&self) -> Rkey<&str>
    where
        S: AsRef<str>,
    {
        // SAFETY: self is already validated.
        unsafe { Rkey::unchecked(self.0.as_ref()) }
    }
}

impl<S: Bos<str> + AsRef<str>> Rkey<S> {
    /// Fallible constructor, validates, wraps the input directly.
    pub fn new(s: S) -> Result<Self, AtStrError> {
        validate_rkey(s.as_ref())?;
        Ok(Self(s))
    }

    /// Infallible constructor. Panics on invalid rkeys.
    pub fn raw(s: S) -> Self {
        Self::new(s).expect("invalid rkey")
    }
}

impl<S: Bos<str> + FromStr> Rkey<S> {
    /// Fallible constructor, validates, takes ownership.
    pub fn new_owned(rkey: impl AsRef<str>) -> Result<Self, AtStrError> {
        let rkey = rkey.as_ref();
        validate_rkey(rkey)?;
        let s = S::from_str(rkey).map_err(|_| {
            AtStrError::new("record-key", rkey.to_string(), StrParseKind::Conversion)
        })?;
        Ok(Self(s))
    }

    /// Fallible constructor for static strings.
    pub fn new_static(rkey: &'static str) -> Result<Self, AtStrError> {
        validate_rkey(rkey)?;
        let s = S::from_str(rkey).map_err(|_| {
            AtStrError::new("record-key", rkey.to_string(), StrParseKind::Conversion)
        })?;
        Ok(Self(s))
    }
}

impl<T> Bos<str> for RecordKey<T>
where
    T: RecordKeyType + Bos<str> + AsRef<str>,
{
    type Ref<'this>
        = &'this str
    where
        Self: 'this;

    fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
        this.as_ref()
    }
}

impl<'de, S> Deserialize<'de> for Rkey<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        validate_rkey(s.as_ref()).map_err(D::Error::custom)?;
        Ok(Rkey(s))
    }
}

impl<S: Bos<str> + IntoStatic> IntoStatic for Rkey<S>
where
    S::Output: Bos<str>,
{
    type Output = Rkey<S::Output>;

    fn into_static(self) -> Self::Output {
        Rkey(self.0.into_static())
    }
}

impl<S: Bos<str>> Rkey<S> {
    /// Convert to an `Rkey` with a different backing type.
    pub fn convert<B: Bos<str> + From<S>>(self) -> Rkey<B> {
        Rkey(B::from(self.0))
    }
}

impl FromStr for Rkey {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for Rkey<CowStr<'static>> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl FromStr for Rkey<String> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for Rkey<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Debug for Rkey<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "record-key:{}", self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> From<Rkey<S>> for String {
    fn from(value: Rkey<S>) -> Self {
        value.as_str().to_string()
    }
}

impl<S: Bos<str> + AsRef<str>> From<Rkey<S>> for SmolStr {
    fn from(value: Rkey<S>) -> Self {
        value.as_str().to_smolstr()
    }
}

impl From<String> for Rkey {
    fn from(value: String) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<'r> From<CowStr<'r>> for Rkey<CowStr<'r>> {
    fn from(value: CowStr<'r>) -> Self {
        Self::new(value).unwrap()
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for Rkey<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for Rkey<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<S> Bos<str> for Rkey<S>
where
    S: Bos<str> + AsRef<str>,
{
    type Ref<'this>
        = &'this str
    where
        Self: 'this;

    fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
        this.as_str()
    }
}

/// ATProto Record Key (type `literal:<value>`)
/// Zero-sized type, literal is associated constant of type parameter
///
/// TODO: macro to construct arbitrary ones of these and the associated marker struct
#[derive(Clone, PartialEq, Eq, Serialize, Hash)]
pub struct LiteralKey<T: Literal = SelfRecord> {
    literal: PhantomData<T>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
/// Key for a record where only one of an NSID is supposed to exist
pub struct SelfRecord;

impl Literal for SelfRecord {
    const LITERAL: &'static str = "self";
}

unsafe impl<T: Literal> RecordKeyType for LiteralKey<T> {
    fn as_str(&self) -> &str {
        T::LITERAL
    }
}

/// AT Protocol rkey
impl<T: Literal> LiteralKey<T> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(rkey: impl AsRef<str>) -> Result<Self, &'static str> {
        let rkey = rkey.as_ref();
        if !rkey.eq_ignore_ascii_case(T::LITERAL) {
            Err("Invalid literal rkey - does not match literal")
        } else if [".", ".."].contains(&rkey) {
            Err("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(rkey) {
            Err("Invalid rkey")
        } else {
            Ok(Self {
                literal: PhantomData,
            })
        }
    }

    /// Infallible constructor for when you *know* the string is a valid rkey.
    /// Will panic on invalid rkeys. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The `From<String>` and `From<CowStr>` impls use the same logic.
    pub fn raw(rkey: &str) -> Self {
        if !rkey.eq_ignore_ascii_case(T::LITERAL) {
            panic!(
                "Invalid literal rkey - does not match literal {}",
                T::LITERAL
            )
        } else if [".", ".."].contains(&rkey.as_ref()) {
            panic!("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(rkey) {
            panic!("Invalid rkey")
        } else {
            Self {
                literal: PhantomData,
            }
        }
    }

    /// Infallible type constructor
    ///
    /// # Safety
    /// Does not validate that the literal is a valid record key
    pub unsafe fn t() -> Self {
        Self {
            literal: PhantomData,
        }
    }

    /// Get the literal record key as a string slice
    pub fn as_str(&self) -> &str {
        T::LITERAL
    }
}

impl<T: Literal> FromStr for LiteralKey<T> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de, T: Literal> Deserialize<'de> for LiteralKey<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl<T: Literal> fmt::Display for LiteralKey<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::LITERAL)
    }
}

impl<T: Literal> fmt::Debug for LiteralKey<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "literal:{}", T::LITERAL)
    }
}

impl<'r, T: Literal> From<LiteralKey<T>> for String {
    fn from(_value: LiteralKey<T>) -> Self {
        T::LITERAL.to_string()
    }
}

impl<'r, T: Literal> From<LiteralKey<T>> for CowStr<'r> {
    fn from(_value: LiteralKey<T>) -> Self {
        CowStr::Borrowed(T::LITERAL)
    }
}

impl<T: Literal> TryFrom<String> for LiteralKey<T> {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !value.eq_ignore_ascii_case(T::LITERAL) {
            Err("Invalid literal rkey - does not match literal")
        } else if [".", ".."].contains(&value.as_str()) {
            Err("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(&value) {
            Err("Invalid rkey")
        } else {
            Ok(Self {
                literal: PhantomData,
            })
        }
    }
}

impl<'r, T: Literal> TryFrom<CowStr<'r>> for LiteralKey<T> {
    type Error = &'static str;
    fn try_from(value: CowStr<'r>) -> Result<Self, Self::Error> {
        if !value.eq_ignore_ascii_case(T::LITERAL) {
            Err("Invalid literal rkey - does not match literal")
        } else if [".", ".."].contains(&value.as_ref()) {
            Err("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(&value) {
            Err("Invalid rkey")
        } else {
            Ok(Self {
                literal: PhantomData,
            })
        }
    }
}

impl<T: Literal> AsRef<str> for LiteralKey<T> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<T: Literal> Deref for LiteralKey<T> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_rkeys() {
        assert!(Rkey::new("3jzfcijpj2z2a").is_ok()); // TID format
        assert!(Rkey::new("self").is_ok()); // literal
        assert!(Rkey::new("com.example.foo").is_ok()); // NSID format
        assert!(Rkey::new("foo-bar_baz").is_ok());
        assert!(Rkey::new("foo:bar").is_ok());
        assert!(Rkey::new("foo~bar").is_ok());
    }

    #[test]
    fn length_constraints() {
        assert!(Rkey::new("a").is_ok()); // min 1
        let valid_512 = "a".repeat(512);
        assert_eq!(valid_512.len(), 512);
        assert!(Rkey::new(valid_512).is_ok());

        let too_long_513 = "a".repeat(513);
        assert_eq!(too_long_513.len(), 513);
        assert!(Rkey::new(too_long_513).is_err());
    }

    #[test]
    fn disallowed_literals() {
        assert!(Rkey::new(".").is_err());
        assert!(Rkey::new("..").is_err());
        assert!(Rkey::new("...").is_ok()); // 3+ dots is fine
    }

    #[test]
    fn allowed_characters() {
        assert!(Rkey::new("abc123").is_ok());
        assert!(Rkey::new("ABC123").is_ok());
        assert!(Rkey::new("foo-bar").is_ok());
        assert!(Rkey::new("foo_bar").is_ok());
        assert!(Rkey::new("foo.bar").is_ok());
        assert!(Rkey::new("foo:bar").is_ok());
        assert!(Rkey::new("foo~bar").is_ok());
    }

    #[test]
    fn disallowed_characters() {
        assert!(Rkey::new("foo bar").is_err());
        assert!(Rkey::new("foo@bar").is_err());
        assert!(Rkey::new("foo#bar").is_err());
        assert!(Rkey::new("foo/bar").is_err());
        assert!(Rkey::new("foo\\bar").is_err());
    }

    #[test]
    fn literal_key_self() {
        let key = LiteralKey::<SelfRecord>::new("self").unwrap();
        assert_eq!(key.as_str(), "self");

        assert!(LiteralKey::<SelfRecord>::new("Self").is_ok()); // case insensitive
        assert!(LiteralKey::<SelfRecord>::new("other").is_err());
    }

    #[test]
    fn literal_key_disallowed() {
        assert!(LiteralKey::<SelfRecord>::new(".").is_err());
        assert!(LiteralKey::<SelfRecord>::new("..").is_err());
    }
}
