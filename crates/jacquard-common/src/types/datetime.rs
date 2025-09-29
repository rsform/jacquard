use chrono::DurationRound;
use serde::Serializer;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::ToSmolStr;
use std::sync::LazyLock;
use std::{cmp, str::FromStr};

use crate::{CowStr, IntoStatic};
use regex::Regex;

pub static ISO8601_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|(\+[0-9]{2}|\-[0-9][1-9]):[0-9]{2})$").unwrap()
});

/// A Lexicon timestamp.
#[derive(Clone, Debug, Eq, Hash)]
pub struct Datetime {
    /// Serialized form. Preserved during parsing to ensure round-trip re-serialization.
    serialized: CowStr<'static>,
    /// Parsed form.
    dt: chrono::DateTime<chrono::FixedOffset>,
}

impl PartialEq for Datetime {
    fn eq(&self, other: &Self) -> bool {
        self.dt == other.dt
    }
}

impl Ord for Datetime {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.dt.cmp(&other.dt)
    }
}

impl PartialOrd for Datetime {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Datetime {
    /// Returns a `Datetime` which corresponds to the current date and time in UTC.
    ///
    /// The timestamp uses microsecond precision.
    pub fn now() -> Self {
        Self::new(chrono::Utc::now().fixed_offset())
    }

    /// Constructs a new Lexicon timestamp.
    ///
    /// The timestamp is rounded to microsecond precision.
    pub fn new(dt: chrono::DateTime<chrono::FixedOffset>) -> Self {
        let dt = dt
            .duration_round(chrono::Duration::microseconds(1))
            .expect("delta does not exceed limits");
        // This serialization format is compatible with ISO 8601.
        let serialized = CowStr::Owned(
            dt.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                .to_smolstr(),
        );
        Self { serialized, dt }
    }

    /// Infallibly parses a new Lexicon timestamp from a compatible str reference
    ///
    /// Panics if invalid. Use the fallible trait implementations or deserialize for input
    /// you cannot reasonably trust to be properly formatted.
    pub fn raw_str(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        if ISO8601_REGEX.is_match(s) {
            let dt = chrono::DateTime::parse_from_rfc3339(s).expect("valid ISO8601 time string");
            Self {
                serialized: CowStr::Borrowed(s).into_static(),
                dt,
            }
        } else {
            panic!("atproto datetime should be valid ISO8601")
        }
    }

    /// Extracts a string slice containing the entire `Datetime`.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.serialized.as_ref()
    }
}

impl FromStr for Datetime {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // The `chrono` crate only supports RFC 3339 parsing, but Lexicon restricts
        // datetimes to the subset that is also valid under ISO 8601. Apply a regex that
        // validates enough of the relevant ISO 8601 format that the RFC 3339 parser can
        // do the rest.
        if ISO8601_REGEX.is_match(s) {
            let dt = chrono::DateTime::parse_from_rfc3339(s)?;
            Ok(Self {
                serialized: CowStr::Owned(s.to_smolstr()),
                dt,
            })
        } else {
            // Simulate an invalid `ParseError`.
            Err(chrono::DateTime::parse_from_rfc3339("invalid").expect_err("invalid"))
        }
    }
}

impl<'de> Deserialize<'de> for Datetime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: String = Deserialize::deserialize(deserializer)?;
        Self::from_str(&value).map_err(D::Error::custom)
    }
}
impl Serialize for Datetime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.serialized)
    }
}

impl AsRef<chrono::DateTime<chrono::FixedOffset>> for Datetime {
    fn as_ref(&self) -> &chrono::DateTime<chrono::FixedOffset> {
        &self.dt
    }
}

impl TryFrom<String> for Datetime {
    type Error = chrono::ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if ISO8601_REGEX.is_match(&value) {
            let dt = chrono::DateTime::parse_from_rfc3339(&value)?;
            Ok(Self {
                serialized: CowStr::Owned(value.to_smolstr()),
                dt,
            })
        } else {
            // Simulate an invalid `ParseError`.
            Err(chrono::DateTime::parse_from_rfc3339("invalid").expect_err("invalid"))
        }
    }
}

impl TryFrom<CowStr<'_>> for Datetime {
    type Error = chrono::ParseError;
    fn try_from(value: CowStr<'_>) -> Result<Self, Self::Error> {
        if ISO8601_REGEX.is_match(&value) {
            let dt = chrono::DateTime::parse_from_rfc3339(&value)?;
            Ok(Self {
                serialized: value.into_static(),
                dt,
            })
        } else {
            // Simulate an invalid `ParseError`.
            Err(chrono::DateTime::parse_from_rfc3339("invalid").expect_err("invalid"))
        }
    }
}
