use crate::types::Literal;
use crate::types::string::AtStrError;
use crate::{CowStr, IntoStatic};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::marker::PhantomData;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

/// Trait for typed record key implementations
///
/// Allows different record key types (TID, NSID, literals, generic strings) while
/// maintaining validation guarantees. Implementers must ensure compliance with the
/// AT Protocol [record key specification](https://atproto.com/specs/record-key).
///
/// # Safety
/// Implementations must ensure the string representation matches [`RKEY_REGEX`] and
/// is not "." or "..". Built-in implementations: `Tid`, `Nsid`, `Literal<T>`, `Rkey<'_>`.
pub unsafe trait RecordKeyType: Clone + Serialize {
    /// Get the record key as a string slice
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

impl<'a> RecordKey<Rkey<'a>> {
    /// Create a new `RecordKey` from a string slice
    pub fn any(str: &'a str) -> Result<Self, AtStrError> {
        Ok(RecordKey(Rkey::new(str)?))
    }

    /// Create a new `RecordKey` from a CowStr
    pub fn any_cow(str: CowStr<'a>) -> Result<Self, AtStrError> {
        Ok(RecordKey(Rkey::new_cow(str)?))
    }

    /// Create a new `RecordKey` from a static string slice
    pub fn any_static(str: &'static str) -> Result<Self, AtStrError> {
        Ok(RecordKey(Rkey::new_static(str)?))
    }
}

impl<T> From<T> for RecordKey<Rkey<'_>>
where
    T: RecordKeyType,
{
    fn from(value: T) -> Self {
        RecordKey(Rkey::from_str(value.as_str()).expect("Invalid rkey"))
    }
}

impl FromStr for RecordKey<Rkey<'_>> {
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
#[derive(Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Rkey<'r>(pub(crate) CowStr<'r>);

unsafe impl<'r> RecordKeyType for Rkey<'r> {
    fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

/// Regex for record key validation per AT Protocol spec
pub static RKEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9.\-_:~]{1,512}$").unwrap());

impl<'r> Rkey<'r> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(rkey: &'r str) -> Result<Self, AtStrError> {
        if [".", ".."].contains(&rkey) {
            Err(AtStrError::disallowed("record-key", rkey, &[".", ".."]))
        } else if !RKEY_REGEX.is_match(rkey) {
            Err(AtStrError::regex(
                "record-key",
                rkey,
                SmolStr::new_static("doesn't match 'any' schema"),
            ))
        } else {
            Ok(Self(CowStr::Borrowed(rkey)))
        }
    }

    /// Fallible constructor, validates, takes ownership
    pub fn new_owned(rkey: impl AsRef<str>) -> Result<Self, AtStrError> {
        let rkey = rkey.as_ref();
        if [".", ".."].contains(&rkey) {
            Err(AtStrError::disallowed("record-key", rkey, &[".", ".."]))
        } else if !RKEY_REGEX.is_match(rkey) {
            Err(AtStrError::regex(
                "record-key",
                rkey,
                SmolStr::new_static("doesn't match 'any' schema"),
            ))
        } else {
            Ok(Self(CowStr::Owned(rkey.to_smolstr())))
        }
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(rkey: &'static str) -> Result<Self, AtStrError> {
        if [".", ".."].contains(&rkey) {
            Err(AtStrError::disallowed("record-key", rkey, &[".", ".."]))
        } else if !RKEY_REGEX.is_match(rkey) {
            Err(AtStrError::regex(
                "record-key",
                rkey,
                SmolStr::new_static("doesn't match 'any' schema"),
            ))
        } else {
            Ok(Self(CowStr::new_static(rkey)))
        }
    }

    /// Fallible constructor, validates, borrows from input if possible
    pub fn new_cow(rkey: CowStr<'r>) -> Result<Self, AtStrError> {
        if [".", ".."].contains(&rkey.as_ref()) {
            Err(AtStrError::disallowed("record-key", &rkey, &[".", ".."]))
        } else if !RKEY_REGEX.is_match(&rkey) {
            Err(AtStrError::regex(
                "record-key",
                &rkey,
                SmolStr::new_static("doesn't match 'any' schema"),
            ))
        } else {
            Ok(Self(rkey))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid rkey.
    /// Will panic on invalid rkeys. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From impls use the same logic.
    pub fn raw(rkey: &'r str) -> Self {
        if [".", ".."].contains(&rkey) {
            panic!("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(rkey) {
            panic!("Invalid rkey")
        } else {
            Self(CowStr::Borrowed(rkey))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid rkey.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(rkey: &'r str) -> Self {
        Self(CowStr::Borrowed(rkey))
    }

    /// Get the record key as a string slice
    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl<'r> FromStr for Rkey<'r> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if [".", ".."].contains(&s) {
            Err(AtStrError::disallowed("record-key", s, &[".", ".."]))
        } else if !RKEY_REGEX.is_match(s) {
            Err(AtStrError::regex(
                "record-key",
                s,
                SmolStr::new_static("doesn't match 'any' schema"),
            ))
        } else {
            Ok(Self(CowStr::Owned(s.to_smolstr())))
        }
    }
}

impl IntoStatic for Rkey<'_> {
    type Output = Rkey<'static>;

    fn into_static(self) -> Self::Output {
        Rkey(self.0.into_static())
    }
}

impl<'de, 'a> Deserialize<'de> for Rkey<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new_cow(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Rkey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Rkey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "record-key:{}", self.0)
    }
}

impl From<Rkey<'_>> for String {
    fn from(value: Rkey<'_>) -> Self {
        value.0.to_string()
    }
}

impl<'r> From<Rkey<'r>> for CowStr<'r> {
    fn from(value: Rkey<'r>) -> Self {
        value.0
    }
}

impl<'r> From<Rkey<'r>> for SmolStr {
    fn from(value: Rkey) -> Self {
        value.0.to_smolstr()
    }
}

impl<'r> From<String> for Rkey<'r> {
    fn from(value: String) -> Self {
        if [".", ".."].contains(&value.as_str()) {
            panic!("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(&value) {
            panic!("Invalid rkey")
        } else {
            Self(CowStr::Owned(value.to_smolstr()))
        }
    }
}

impl<'r> From<CowStr<'r>> for Rkey<'r> {
    fn from(value: CowStr<'r>) -> Self {
        if [".", ".."].contains(&value.as_ref()) {
            panic!("Disallowed rkey")
        } else if !RKEY_REGEX.is_match(&value) {
            panic!("Invalid rkey")
        } else {
            Self(value)
        }
    }
}

impl AsRef<str> for Rkey<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Rkey<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
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
        assert!(Rkey::new(&valid_512).is_ok());

        let too_long_513 = "a".repeat(513);
        assert_eq!(too_long_513.len(), 513);
        assert!(Rkey::new(&too_long_513).is_err());
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
