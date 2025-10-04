use crate::types::string::AtStrError;
use crate::{CowStr, IntoStatic};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, de::Error};
use smol_str::{SmolStr, ToSmolStr};
use std::fmt;
use std::sync::LazyLock;
use std::{ops::Deref, str::FromStr};

#[derive(Clone, PartialEq, Eq, Serialize, Hash)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Did<'d>(CowStr<'d>);

/// Regex for DID validation per AT Protocol spec.
///
/// Note: This regex allows `%` in the identifier but prevents DIDs from ending with `:` or `%`.
/// It does NOT validate that percent-encoding is well-formed (i.e., `%XX` where XX are hex digits).
/// This matches the behavior of the official TypeScript implementation, which also does not
/// enforce percent-encoding validity at validation time. While the spec states "percent sign
/// must be followed by two hex characters," this is treated as a best practice rather than
/// a hard validation requirement.
pub static DID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^did:[a-z]+:[a-zA-Z0-9._:%-]*[a-zA-Z0-9._-]$").unwrap());

impl<'d> Did<'d> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(did: &'d str) -> Result<Self, AtStrError> {
        let did = did.strip_prefix("at://").unwrap_or(did);
        if did.len() > 2048 {
            Err(AtStrError::too_long("did", did, 2048, did.len()))
        } else if !DID_REGEX.is_match(did) {
            Err(AtStrError::regex(
                "did",
                did,
                SmolStr::new_static("invalid"),
            ))
        } else {
            Ok(Self(CowStr::Borrowed(did)))
        }
    }

    /// Fallible constructor, validates, takes ownership
    pub fn new_owned(did: impl AsRef<str>) -> Result<Self, AtStrError> {
        let did = did.as_ref();
        let did = did.strip_prefix("at://").unwrap_or(did);
        if did.len() > 2048 {
            Err(AtStrError::too_long("did", did, 2048, did.len()))
        } else if !DID_REGEX.is_match(did) {
            Err(AtStrError::regex(
                "did",
                did,
                SmolStr::new_static("invalid"),
            ))
        } else {
            Ok(Self(CowStr::Owned(did.to_smolstr())))
        }
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(did: &'static str) -> Result<Self, AtStrError> {
        let did = did.strip_prefix("at://").unwrap_or(did);
        if did.len() > 2048 {
            Err(AtStrError::too_long("did", did, 2048, did.len()))
        } else if !DID_REGEX.is_match(did) {
            Err(AtStrError::regex(
                "did",
                did,
                SmolStr::new_static("invalid"),
            ))
        } else {
            Ok(Self(CowStr::new_static(did)))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid DID.
    /// Will panic on invalid DIDs. If you're manually decoding atproto records
    /// or API values you know are valid (rather than using serde), this is the one to use.
    /// The `From<String>` and `From<CowStr>` impls use the same logic.
    pub fn raw(did: &'d str) -> Self {
        let did = did.strip_prefix("at://").unwrap_or(did);
        if did.len() > 2048 {
            panic!("DID too long")
        } else if !DID_REGEX.is_match(did) {
            panic!("Invalid DID")
        } else {
            Self(CowStr::Borrowed(did))
        }
    }

    /// Infallible constructor for when you *know* the string is a valid DID.
    /// Marked unsafe because responsibility for upholding the invariant is on the developer.
    pub unsafe fn unchecked(did: &'d str) -> Self {
        Self(CowStr::Borrowed(did))
    }

    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for Did<'_> {
    type Err = AtStrError;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    /// Prefer `Did::new()` or `Did::raw` if you want to borrow.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl IntoStatic for Did<'_> {
    type Output = Did<'static>;

    fn into_static(self) -> Self::Output {
        Did(self.0.into_static())
    }
}

impl<'de, 'a> Deserialize<'de> for Did<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for Did<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Did<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}", self.0)
    }
}

impl<'d> From<Did<'d>> for String {
    fn from(value: Did<'d>) -> Self {
        value.0.to_string()
    }
}

impl<'d> From<Did<'d>> for CowStr<'d> {
    fn from(value: Did<'d>) -> Self {
        value.0
    }
}

impl From<String> for Did<'static> {
    fn from(value: String) -> Self {
        let value = if let Some(did) = value.strip_prefix("at://") {
            CowStr::Borrowed(did)
        } else {
            value.into()
        };
        if value.len() > 2048 {
            panic!("DID too long")
        } else if !DID_REGEX.is_match(&value) {
            panic!("Invalid DID")
        } else {
            Self(value.into_static())
        }
    }
}

impl<'d> From<CowStr<'d>> for Did<'d> {
    fn from(value: CowStr<'d>) -> Self {
        let value = if let Some(did) = value.strip_prefix("at://") {
            CowStr::Borrowed(did)
        } else {
            value
        };
        if value.len() > 2048 {
            panic!("DID too long")
        } else if !DID_REGEX.is_match(&value) {
            panic!("Invalid DID")
        } else {
            Self(value.into_static())
        }
    }
}

impl AsRef<str> for Did<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Did<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_dids() {
        assert!(Did::new("did:plc:abc123").is_ok());
        assert!(Did::new("did:web:example.com").is_ok());
        assert!(Did::new("did:method:val_ue").is_ok());
        assert!(Did::new("did:method:val-ue").is_ok());
        assert!(Did::new("did:method:val.ue").is_ok());
        assert!(Did::new("did:method:val%20ue").is_ok());
    }

    #[test]
    fn prefix_stripping() {
        assert_eq!(
            Did::new("at://did:plc:foo").unwrap().as_str(),
            "did:plc:foo"
        );
        assert_eq!(Did::new("did:plc:foo").unwrap().as_str(), "did:plc:foo");
    }

    #[test]
    fn must_start_with_did() {
        assert!(Did::new("DID:plc:foo").is_err());
        assert!(Did::new("plc:foo").is_err());
        assert!(Did::new("foo").is_err());
    }

    #[test]
    fn method_must_be_lowercase() {
        assert!(Did::new("did:plc:foo").is_ok());
        assert!(Did::new("did:PLC:foo").is_err());
        assert!(Did::new("did:Plc:foo").is_err());
    }

    #[test]
    fn cannot_end_with_colon_or_percent() {
        assert!(Did::new("did:plc:foo:").is_err());
        assert!(Did::new("did:plc:foo%").is_err());
        assert!(Did::new("did:plc:foo:bar").is_ok());
    }

    #[test]
    fn max_length() {
        let valid_2048 = format!("did:plc:{}", "a".repeat(2048 - 8));
        assert_eq!(valid_2048.len(), 2048);
        assert!(Did::new(&valid_2048).is_ok());

        let too_long_2049 = format!("did:plc:{}", "a".repeat(2049 - 8));
        assert_eq!(too_long_2049.len(), 2049);
        assert!(Did::new(&too_long_2049).is_err());
    }

    #[test]
    fn allowed_characters() {
        assert!(Did::new("did:method:abc123").is_ok());
        assert!(Did::new("did:method:ABC123").is_ok());
        assert!(Did::new("did:method:a_b_c").is_ok());
        assert!(Did::new("did:method:a-b-c").is_ok());
        assert!(Did::new("did:method:a.b.c").is_ok());
        assert!(Did::new("did:method:a:b:c").is_ok());
    }

    #[test]
    fn disallowed_characters() {
        assert!(Did::new("did:method:a b").is_err());
        assert!(Did::new("did:method:a@b").is_err());
        assert!(Did::new("did:method:a#b").is_err());
        assert!(Did::new("did:method:a?b").is_err());
    }

    #[test]
    fn percent_encoding() {
        // Valid percent encoding
        assert!(Did::new("did:method:foo%20bar").is_ok());
        assert!(Did::new("did:method:foo%2Fbar").is_ok());

        // DIDs cannot end with %
        assert!(Did::new("did:method:foo%").is_err());

        // IMPORTANT: The regex does NOT validate that percent-encoding is well-formed.
        // This matches the TypeScript reference implementation's behavior.
        // While the spec says "percent sign must be followed by two hex characters",
        // implementations treat this as a best practice, not a hard validation requirement.
        // Thus, malformed percent encoding like %2x is accepted by the regex.
        assert!(Did::new("did:method:foo%2x").is_ok());
        assert!(Did::new("did:method:foo%ZZ").is_ok());
    }
}
