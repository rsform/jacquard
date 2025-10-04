use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, SmolStrBuilder};
use std::fmt;
use std::sync::LazyLock;
use std::time::SystemTime;
use std::{ops::Deref, str::FromStr};

use crate::CowStr;
use crate::types::integer::LimitedU32;
use crate::types::string::{AtStrError, StrParseKind};
use regex::Regex;

const S32_CHAR: &str = "234567abcdefghijklmnopqrstuvwxyz";

fn s32_encode(mut i: u64) -> SmolStr {
    let mut s = SmolStrBuilder::new();
    for _ in 0..13 {
        let c = i & 0x1F;
        s.push(S32_CHAR.chars().nth(c as usize).unwrap());

        i >>= 5;
    }

    let mut builder = SmolStrBuilder::new();
    for c in s.finish().chars().rev() {
        builder.push(c);
    }
    builder.finish()
}

/// Regex for TID validation per AT Protocol spec
static TID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[234567abcdefghij][234567abcdefghijklmnopqrstuvwxyz]{12}$").unwrap()
});

/// Timestamp Identifier (TID) for record keys and commit revisions
///
/// TIDs are compact, sortable identifiers based on timestamps. They're used as record keys
/// and repository commit revision numbers in AT Protocol.
///
/// Format:
/// - Always 13 ASCII characters
/// - Base32-sortable encoding (`234567abcdefghijklmnopqrstuvwxyz`)
/// - First 53 bits: microseconds since UNIX epoch
/// - Final 10 bits: random clock identifier for collision resistance
///
/// TIDs are sortable by timestamp and suitable for use in URLs. Generate new TIDs with
/// `Tid::now()` or `Tid::now_with_clock_id()`.
///
/// See: <https://atproto.com/specs/tid>
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Tid(SmolStr);

impl Tid {
    /// Parses a `TID` from the given string.
    pub fn new(tid: impl AsRef<str>) -> Result<Self, AtStrError> {
        let tid = tid.as_ref();
        if tid.len() != 13 {
            let kind = if tid.len() > 13 {
                StrParseKind::TooLong {
                    max: 13,
                    actual: tid.len(),
                }
            } else {
                StrParseKind::TooShort {
                    min: 13,
                    actual: tid.len(),
                }
            };
            Err(AtStrError::new("tid", tid.to_string(), kind))
        } else if !TID_REGEX.is_match(&tid.as_ref()) {
            let kind = StrParseKind::RegexFail {
                span: None,
                message: SmolStr::new_static("didn't match schema"),
            };
            Err(AtStrError::new("tid", tid.to_string(), kind))
        } else {
            Ok(Self(SmolStr::new_inline(&tid)))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid TID.
    /// Will panic on invalid TID. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The `From<String>` and `From<CowStr>` impls use the same logic.
    pub fn raw(tid: impl AsRef<str>) -> Self {
        let tid = tid.as_ref();
        if tid.len() != 13 {
            panic!("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&tid) {
            panic!("Invalid TID")
        } else {
            Self(SmolStr::new_inline(tid))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid TID.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(tid: impl AsRef<str>) -> Self {
        let tid = tid.as_ref();
        Self(SmolStr::new_inline(tid))
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

    /// Construct a TID from a timestamp (in microseconds) and clock ID
    pub fn from_time(timestamp: usize, clkid: u32) -> Self {
        let str = smol_str::format_smolstr!(
            "{0}{1:2>2}",
            s32_encode(timestamp as u64),
            s32_encode(Into::<u32>::into(clkid) as u64)
        );
        Self(str)
    }

    /// Extract the timestamp component (microseconds since UNIX epoch)
    pub fn timestamp(&self) -> usize {
        s32decode(self.0[0..11].to_owned())
    }

    /// Compare two TIDs chronologically (newer > older)
    ///
    /// Returns 1 if self is newer, -1 if older, 0 if equal
    pub fn compare_to(&self, other: &Tid) -> i8 {
        if self.0 > other.0 {
            return 1;
        }
        if self.0 < other.0 {
            return -1;
        }
        0
    }

    /// Check if this TID is newer than another
    pub fn newer_than(&self, other: &Tid) -> bool {
        self.compare_to(other) > 0
    }

    /// Check if this TID is older than another
    pub fn older_than(&self, other: &Tid) -> bool {
        self.compare_to(other) < 0
    }

    /// Generate the next TID in sequence after the given TID
    pub fn next_str(prev: Option<Tid>) -> Result<Self, AtStrError> {
        let prev = match prev {
            None => None,
            Some(prev) => Some(Tid::new(prev)?),
        };
        Ok(Ticker::new().next(prev))
    }

    /// Construct a new [Tid] that represents the current time.
    ///
    /// If you have multiple clock sources, you can use `clkid` to distinguish between them
    /// and hint to other implementations that the timestamp cannot be compared with other
    /// timestamps from other sources.
    /// If you are only using a single clock source, you can just specify `0` for `clkid`.
    ///
    /// TODO: fix to auto-increment if it would return the same value twice
    pub fn now(clkid: LimitedU32<1023>) -> Self {
        Self::from_datetime(clkid, chrono::Utc::now())
    }

    /// Construct a new [Tid] that represents the current time with clkid 0.
    ///
    /// TODO: fix to auto-increment if it would return the same value twice
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

/// Decode a base32-sortable string into a usize
pub fn s32decode(s: String) -> usize {
    let mut i: usize = 0;
    for c in s.chars() {
        i = i * 32 + S32_CHAR.chars().position(|x| x == c).unwrap();
    }
    i
}

impl FromStr for Tid {
    type Err = AtStrError;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `Did::new()` or `Did::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for Tid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Tid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Tid> for String {
    fn from(value: Tid) -> Self {
        value.0.to_string()
    }
}

impl From<Tid> for SmolStr {
    fn from(value: Tid) -> Self {
        value.0
    }
}

impl crate::IntoStatic for Tid {
    type Output = Tid;

    fn into_static(self) -> Self::Output {
        self
    }
}

impl From<String> for Tid {
    fn from(value: String) -> Self {
        if value.len() != 13 {
            panic!("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&value) {
            panic!("Invalid TID")
        } else {
            Self(SmolStr::new_inline(&value))
        }
    }
}

impl<'t> From<CowStr<'t>> for Tid {
    fn from(value: CowStr<'t>) -> Self {
        if value.len() != 13 {
            panic!("TID must be 13 characters")
        } else if !TID_REGEX.is_match(&value) {
            panic!("Invalid TID")
        } else {
            Self(SmolStr::new_inline(&value))
        }
    }
}

impl AsRef<str> for Tid {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Tid {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// Based on adenosine/adenosine/src/identifiers.rs
/// TODO: clean up and normalize stuff between this and the stuff pulled from atrium
pub struct Ticker {
    last_timestamp: usize,
    clock_id: u32,
}

impl Ticker {
    /// Create a new TID generator with random clock ID
    pub fn new() -> Self {
        let mut ticker = Self {
            last_timestamp: 0,
            // mask to 10 bits
            clock_id: rand::random::<u32>() & 0x03FF,
        };
        // prime the pump
        ticker.next(None);
        ticker
    }

    /// Generate the next TID, optionally ensuring it's after the given TID
    pub fn next(&mut self, prev: Option<Tid>) -> Tid {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("timestamp in micros since UNIX epoch")
            .as_micros() as usize;
        // mask to 53 bits
        let now = now & 0x001FFFFFFFFFFFFF;
        if now > self.last_timestamp {
            self.last_timestamp = now;
        } else {
            self.last_timestamp += 1;
        }
        // 53 bits of millis
        let micros = self.last_timestamp & 0x001FFFFFFFFFFFFF;
        // 10 bits of clock ID
        let clock_id = self.clock_id & 0x03FF;

        let tid = Tid::from_time(micros, clock_id as u32);
        match prev {
            Some(ref prev) if tid.newer_than(prev) => tid,
            Some(prev) => Tid::from_time(prev.timestamp() + 1, clock_id as u32),
            None => tid,
        }
    }
}

impl Default for Ticker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_tids() {
        assert!(Tid::new("3jzfcijpj2z2a").is_ok());
        assert!(Tid::new("2222222222222").is_ok());
        assert!(Tid::new("j7777777777777").is_err()); // j is valid for first char but makes high bit set
    }

    #[test]
    fn exact_length() {
        assert!(Tid::new("3jzfcijpj2z2a").is_ok());
        assert!(Tid::new("3jzfcijpj2z2").is_err()); // 12 chars
        assert!(Tid::new("3jzfcijpj2z2aa").is_err()); // 14 chars
    }

    #[test]
    fn first_char_constraint() {
        // First char must be 2-7 or a-j (not k-z)
        assert!(Tid::new("2222222222222").is_ok());
        assert!(Tid::new("7777777777777").is_ok());
        assert!(Tid::new("a222222222222").is_ok());
        assert!(Tid::new("j222222222222").is_ok());
        assert!(Tid::new("k222222222222").is_err());
        assert!(Tid::new("z222222222222").is_err());
    }

    #[test]
    fn remaining_chars_constraint() {
        // Remaining 12 chars must be 2-7 or a-z
        assert!(Tid::new("3abcdefghijkl").is_ok());
        assert!(Tid::new("3zzzzzzzzzzzz").is_ok());
        assert!(Tid::new("3222222222222").is_ok());
        assert!(Tid::new("3777777777777").is_ok());
    }

    #[test]
    fn disallowed_characters() {
        assert!(Tid::new("3jzfcijpj2z2A").is_err()); // uppercase
        assert!(Tid::new("3jzfcijpj2z21").is_err()); // 1 not allowed
        assert!(Tid::new("3jzfcijpj2z28").is_err()); // 8 not allowed
        assert!(Tid::new("3jzfcijpj2z2-").is_err()); // special char
    }

    #[test]
    fn generation_and_comparison() {
        let tid1 = Tid::now_0();
        std::thread::sleep(std::time::Duration::from_micros(10));
        let tid2 = Tid::now_0();

        assert!(tid1.as_str().len() == 13);
        assert!(tid2.as_str().len() == 13);
        assert!(tid2.newer_than(&tid1));
        assert!(tid1.older_than(&tid2));
    }

    #[test]
    fn ticker_monotonic() {
        let mut ticker = Ticker::new();
        let tid1 = ticker.next(None);
        let tid2 = ticker.next(Some(tid1.clone()));
        let tid3 = ticker.next(Some(tid2.clone()));

        assert!(tid2.newer_than(&tid1));
        assert!(tid3.newer_than(&tid2));
    }
}
