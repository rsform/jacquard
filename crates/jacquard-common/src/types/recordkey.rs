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

/// Trait for generic typed record keys
///
/// This is deliberately public (so that consumers can develop specialized record key types),
/// but is marked as unsafe, because the implementer is expected to uphold the invariants
/// required by this trait, namely compliance with the [spec](https://atproto.com/specs/record-key)
/// as described by [`RKEY_REGEX`](RKEY_REGEX).
///
/// This crate provides implementations for TID, NSID, literals, and generic strings
pub unsafe trait RecordKeyType: Clone + Serialize {
    fn as_str(&self) -> &str;
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Hash, Debug)]
#[serde(transparent)]
#[repr(transparent)]
pub struct RecordKey<T: RecordKeyType>(pub T);

impl<T> From<T> for RecordKey<Rkey<'_>>
where
    T: RecordKeyType,
{
    fn from(value: T) -> Self {
        RecordKey(Rkey::from_str(value.as_str()).expect("Invalid rkey"))
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

/// ATProto Record Key (type `any`)
/// Catch-all for any string meeting the overall Record Key requirements detailed https://atproto.com/specs/record-key
#[derive(Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Rkey<'r>(CowStr<'r>);

unsafe impl<'r> RecordKeyType for Rkey<'r> {
    fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

pub static RKEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9.\-_:~]{1,512}$").unwrap());

/// AT Protocol rkey
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

    /// Fallible constructor, validates, borrows from input
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

impl<'de> Deserialize<'de> for Rkey<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
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
    /// The From<String> and From<CowStr> impls use the same logic.
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
