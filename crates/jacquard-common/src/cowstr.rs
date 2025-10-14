use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{
    borrow::Cow,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};

use crate::IntoStatic;

/// A copy-on-write immutable string type that uses [`SmolStr`] for
/// the "owned" variant.
///
/// The standard [`Cow`] type cannot be used, since
/// `<str as ToOwned>::Owned` is `String`, and not `SmolStr`.
///
/// Shamelessly ported from [merde](https://github.com/bearcove/merde)
#[derive(Clone)]
pub enum CowStr<'s> {
    /// &str varaiant
    Borrowed(&'s str),
    /// Smolstr variant
    Owned(SmolStr),
}

impl CowStr<'static> {
    /// Create a new `CowStr` by copying from a `&str` — this might allocate
    /// if the string is longer than `MAX_INLINE_SIZE`.
    pub fn copy_from_str(s: &str) -> Self {
        Self::Owned(SmolStr::from(s))
    }

    /// Create a new owned `CowStr` from a static &str without allocating
    pub fn new_static(s: &'static str) -> Self {
        Self::Owned(SmolStr::new_static(s))
    }
}

impl<'s> CowStr<'s> {
    #[inline]
    /// Borrow and decode a byte slice as utf8 into a CowStr
    pub fn from_utf8(s: &'s [u8]) -> Result<Self, std::str::Utf8Error> {
        Ok(Self::Borrowed(std::str::from_utf8(s)?))
    }

    #[inline]
    /// Take bytes and decode them as utf8 into an owned CowStr. Might allocate.
    pub fn from_utf8_owned(s: impl AsRef<[u8]>) -> Result<Self, std::str::Utf8Error> {
        Ok(Self::Owned(SmolStr::new(std::str::from_utf8(&s.as_ref())?)))
    }

    #[inline]
    /// Take bytes and decode them as utf8, skipping invalid characters, taking ownership.
    /// Will allocate, uses String::from_utf8_lossy() internally for now.
    pub fn from_utf8_lossy(s: &'s [u8]) -> Self {
        Self::Owned(String::from_utf8_lossy(&s).into())
    }

    /// # Safety
    ///
    /// This function is unsafe because it does not check that the bytes are valid UTF-8.
    #[inline]
    pub unsafe fn from_utf8_unchecked(s: &'s [u8]) -> Self {
        unsafe { Self::Owned(SmolStr::new(std::str::from_utf8_unchecked(s))) }
    }

    /// Returns a reference to the underlying string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            CowStr::Borrowed(s) => s,
            CowStr::Owned(s) => s.as_str(),
        }
    }
}

impl AsRef<str> for CowStr<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        match self {
            CowStr::Borrowed(s) => s,
            CowStr::Owned(s) => s.as_str(),
        }
    }
}

impl Deref for CowStr<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            CowStr::Borrowed(s) => s,
            CowStr::Owned(s) => s.as_str(),
        }
    }
}

impl<'a> From<Cow<'a, str>> for CowStr<'a> {
    #[inline]
    fn from(s: Cow<'a, str>) -> Self {
        match s {
            Cow::Borrowed(s) => CowStr::Borrowed(s),
            #[allow(clippy::useless_conversion)]
            Cow::Owned(s) => CowStr::Owned(s.into()),
        }
    }
}

impl<'s> From<&'s str> for CowStr<'s> {
    #[inline]
    fn from(s: &'s str) -> Self {
        CowStr::Borrowed(s)
    }
}

impl Default for CowStr<'_> {
    #[inline]
    fn default() -> Self {
        CowStr::new_static("")
    }
}

impl From<String> for CowStr<'_> {
    #[inline]
    fn from(s: String) -> Self {
        #[allow(clippy::useless_conversion)]
        CowStr::Owned(s.into())
    }
}

impl From<Box<str>> for CowStr<'_> {
    #[inline]
    fn from(s: Box<str>) -> Self {
        CowStr::Owned(s.into())
    }
}

impl<'s> From<&'s String> for CowStr<'s> {
    #[inline]
    fn from(s: &'s String) -> Self {
        CowStr::Borrowed(s.as_str())
    }
}

impl From<CowStr<'_>> for String {
    #[inline]
    fn from(s: CowStr<'_>) -> Self {
        match s {
            CowStr::Borrowed(s) => s.into(),
            #[allow(clippy::useless_conversion)]
            CowStr::Owned(s) => s.into(),
        }
    }
}

impl From<CowStr<'_>> for SmolStr {
    #[inline]
    fn from(s: CowStr<'_>) -> Self {
        match s {
            CowStr::Borrowed(s) => SmolStr::new(s),
            CowStr::Owned(s) => SmolStr::new(s),
        }
    }
}

impl From<SmolStr> for CowStr<'_> {
    #[inline]
    fn from(s: SmolStr) -> Self {
        CowStr::Owned(s)
    }
}

impl From<CowStr<'_>> for Box<str> {
    #[inline]
    fn from(s: CowStr<'_>) -> Self {
        match s {
            CowStr::Borrowed(s) => s.into(),
            CowStr::Owned(s) => String::from(s).into_boxed_str(),
        }
    }
}

impl<'a> PartialEq<CowStr<'a>> for CowStr<'_> {
    #[inline]
    fn eq(&self, other: &CowStr<'a>) -> bool {
        self.deref() == other.deref()
    }
}

impl PartialEq<&str> for CowStr<'_> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.deref() == *other
    }
}

impl PartialEq<CowStr<'_>> for &str {
    #[inline]
    fn eq(&self, other: &CowStr<'_>) -> bool {
        *self == other.deref()
    }
}

impl PartialEq<String> for CowStr<'_> {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.deref() == other.as_str()
    }
}

impl PartialEq<CowStr<'_>> for String {
    #[inline]
    fn eq(&self, other: &CowStr<'_>) -> bool {
        self.as_str() == other.deref()
    }
}

impl PartialOrd for CowStr<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(match (self, other) {
            (CowStr::Borrowed(s1), CowStr::Borrowed(s2)) => s1.cmp(s2),
            (CowStr::Borrowed(s1), CowStr::Owned(s2)) => s1.cmp(&s2.as_ref()),
            (CowStr::Owned(s1), CowStr::Borrowed(s2)) => s1.as_str().cmp(s2),
            (CowStr::Owned(s1), CowStr::Owned(s2)) => s1.cmp(s2),
        })
    }
}

impl Ord for CowStr<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (CowStr::Borrowed(s1), CowStr::Borrowed(s2)) => s1.cmp(s2),
            (CowStr::Borrowed(s1), CowStr::Owned(s2)) => s1.cmp(&s2.as_ref()),
            (CowStr::Owned(s1), CowStr::Borrowed(s2)) => s1.as_str().cmp(s2),
            (CowStr::Owned(s1), CowStr::Owned(s2)) => s1.cmp(s2),
        }
    }
}

impl Eq for CowStr<'_> {}

impl Hash for CowStr<'_> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state)
    }
}

impl fmt::Debug for CowStr<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::Display for CowStr<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl IntoStatic for CowStr<'_> {
    type Output = CowStr<'static>;

    #[inline]
    fn into_static(self) -> Self::Output {
        match self {
            CowStr::Borrowed(s) => CowStr::Owned((*s).into()),
            CowStr::Owned(s) => CowStr::Owned(s),
        }
    }
}

impl Serialize for CowStr<'_> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self)
    }
}

/// Deserialization helper for things that wrap a CowStr
pub struct CowStrVisitor;

impl<'de> serde::de::Visitor<'de> for CowStrVisitor {
    type Value = CowStr<'de>;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a string")
    }

    #[inline]
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(CowStr::copy_from_str(v))
    }

    #[inline]
    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(CowStr::Borrowed(v))
    }

    #[inline]
    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(v.into())
    }
}

impl<'de, 'a> Deserialize<'de> for CowStr<'a>
where
    'de: 'a,
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<CowStr<'a>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(CowStrVisitor)
    }
}

/// Convert to a CowStr.
pub trait ToCowStr {
    /// Convert to a CowStr.
    fn to_cowstr(&self) -> CowStr<'_>;
}

impl<T> ToCowStr for T
where
    T: fmt::Display + ?Sized,
{
    fn to_cowstr(&self) -> CowStr<'_> {
        CowStr::Owned(smol_str::format_smolstr!("{}", self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partialeq_with_str() {
        let cow_str1 = CowStr::Borrowed("hello");
        let cow_str2 = CowStr::Borrowed("hello");
        let cow_str3 = CowStr::Borrowed("world");

        assert_eq!(cow_str1, "hello");
        assert_eq!("hello", cow_str1);
        assert_eq!(cow_str1, cow_str2);
        assert_ne!(cow_str1, "world");
        assert_ne!("world", cow_str1);
        assert_ne!(cow_str1, cow_str3);
    }
}
