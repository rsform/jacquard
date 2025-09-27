use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Deserializer, Serialize, de::Error};

use crate::types::integer::LimitedU32;
use crate::{CowStr, IntoStatic};
use regex::Regex;

fn s32_encode(mut i: u64) -> CowStr<'static> {
    const S32_CHAR: &[u8] = b"234567abcdefghijklmnopqrstuvwxyz";

    let mut s = CompactString::with_capacity(13);
    for _ in 0..13 {
        let c = i & 0x1F;
        s.push(S32_CHAR[c as usize] as char);

        i >>= 5;
    }

    // Reverse the string to convert it to big-endian format.
    CowStr::Owned(s.chars().rev().collect())
}

static TID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[234567abcdefghij][234567abcdefghijklmnopqrstuvwxyz]{12}$").unwrap()
});

/// A [Timestamp Identifier].
///
/// [Timestamp Identifier]: https://atproto.com/specs/tid
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
pub struct Tid<'t>(CowStr<'t>);

impl<'t> Tid<'t> {
    /// Parses a `TID` from the given string.
    pub fn new(tid: &'t str) -> Result<Self, &'static str> {
        if tid.len() != 13 {
            Err("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&tid) {
            Err("Invalid TID")
        } else {
            Ok(Self(CowStr::Owned(tid.to_compact_string())))
        }
    }

    /// Fallible constructor from an existing CowStr, takes ownership
    pub fn from_cowstr(tid: CowStr<'t>) -> Result<Tid<'t>, &'static str> {
        if tid.len() != 13 {
            Err("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&tid) {
            Err("Invalid TID")
        } else {
            Ok(Self(tid.into_static()))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid TID.
    /// Will panic on invalid TID. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The From<String> and From<CowStr> impls use the same logic.
    pub fn raw(tid: &'t str) -> Self {
        if tid.len() != 13 {
            panic!("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&tid) {
            panic!("Invalid TID")
        } else {
            Self(CowStr::Borrowed(tid))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid TID.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(tid: &'t str) -> Self {
        Self(CowStr::Borrowed(tid))
    }

    /// Construct a new timestamp with the specified clock ID.
    ///
    /// If you have multiple clock sources, you can use `clkid` to distinguish between them
    /// and hint to other implementations that the timestamp cannot be compared with other
    /// timestamps from other sources.
    /// If you are only using a single clock source, you can just specify `0` for `clkid`.
    pub fn from_datetime(clkid: LimitedU32<1023>, time: chrono::DateTime<chrono::Utc>) -> Self {
        let time = time.timestamp_micros() as u64;

        // The TID is laid out as follows:
        // 0TTTTTTTTTTTTTTT TTTTTTTTTTTTTTTT TTTTTTTTTTTTTTTT TTTTTTCCCCCCCCCC
        let tid = (time << 10) & 0x7FFF_FFFF_FFFF_FC00 | (Into::<u32>::into(clkid) as u64 & 0x3FF);
        Self(s32_encode(tid))
    }

    /// Construct a new [Tid] that represents the current time.
    ///
    /// If you have multiple clock sources, you can use `clkid` to distinguish between them
    /// and hint to other implementations that the timestamp cannot be compared with other
    /// timestamps from other sources.
    /// If you are only using a single clock source, you can just specify `0` for `clkid`.
    ///
    /// _Warning:_ It's possible that this function will return the same time more than once.
    /// If it's important that these values be unique, you will want to repeatedly call this
    /// function until a different time is returned.
    pub fn now(clkid: LimitedU32<1023>) -> Self {
        Self::from_datetime(clkid, chrono::Utc::now())
    }

    /// Construct a new [Tid] that represents the current time with clkid 0.
    ///
    /// _Warning:_ It's possible that this function will return the same time more than once.
    /// If it's important that these values be unique, you will want to repeatedly call this
    /// function until a different time is returned.
    pub fn now_0() -> Self {
        Self::from_datetime(LimitedU32::from_str("0").unwrap(), chrono::Utc::now())
    }

    /// Returns the TID as a string slice.
    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Tid<'_> {
    type Err = &'static str;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `Did::new()` or `Did::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_cowstr(CowStr::Borrowed(s).into_static())
    }
}

impl<'de> Deserialize<'de> for Tid<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Tid<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'t> From<Tid<'t>> for String {
    fn from(value: Tid<'t>) -> Self {
        value.0.to_string()
    }
}

impl<'t> From<Tid<'t>> for CowStr<'t> {
    fn from(value: Tid<'t>) -> Self {
        value.0
    }
}

impl From<String> for Tid<'static> {
    fn from(value: String) -> Self {
        if value.len() != 13 {
            panic!("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&value) {
            panic!("Invalid TID")
        } else {
            Self(CowStr::Owned(value.to_compact_string()))
        }
    }
}

impl<'t> From<CowStr<'t>> for Tid<'t> {
    fn from(value: CowStr<'t>) -> Self {
        if value.len() != 13 {
            panic!("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&value) {
            panic!("Invalid TID")
        } else {
            Self(value)
        }
    }
}

impl AsRef<str> for Tid<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Tid<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
