use crate::bos::{Bos, BosStr, DefaultStr};
use crate::types::did::{Did, validate_did};
use crate::types::string::{AtStrError, StrParseKind};
use crate::{CowStr, IntoStatic};
use alloc::string::{String, ToString};
use core::fmt;
use core::ops::Deref;
use core::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize};
use smol_str::SmolStr;

/// A DID audience with an optional service-id fragment.
///
/// Service auth JWTs may target either a bare service DID such as
/// `did:web:example.com` or a DID plus service fragment such as
/// `did:web:example.com#bsky_appview`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct DidService<S: Bos<str> = DefaultStr>(pub(crate) S);

/// Validate a DID service audience string without constructing a [`DidService`].
pub fn validate_did_service(value: &str) -> Result<(), AtStrError> {
    if value.len() > 2048 {
        return Err(AtStrError::too_long(
            "did service audience",
            value,
            2048,
            value.len(),
        ));
    }

    let mut parts = value.split('#');
    let did = parts.next().unwrap_or_default();
    let service = parts.next();
    if parts.next().is_some() {
        return Err(AtStrError::regex(
            "did service audience",
            value,
            SmolStr::new_static("multiple fragments"),
        ));
    }

    validate_did(did)?;

    if let Some(service) = service {
        validate_service_id(service, value)?;
    }

    Ok(())
}

fn validate_service_id(service: &str, whole: &str) -> Result<(), AtStrError> {
    let mut bytes = service.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => {
            return Err(AtStrError::regex(
                "did service audience",
                whole,
                SmolStr::new_static("invalid service id"),
            ));
        }
    }

    if bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
        Ok(())
    } else {
        Err(AtStrError::regex(
            "did service audience",
            whole,
            SmolStr::new_static("invalid service id"),
        ))
    }
}

impl<S: BosStr> DidService<S> {
    /// Get the full DID service audience as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Get the fragmentless DID audience.
    pub fn audience(&self) -> Did<&str> {
        let did = self
            .as_str()
            .split_once('#')
            .map_or(self.as_str(), |(did, _)| did);
        // SAFETY: self is already validated, and validation validates the DID portion.
        unsafe { Did::unchecked(did) }
    }

    /// Get the optional service-id fragment without the leading `#`.
    pub fn service(&self) -> Option<&str> {
        self.as_str().split_once('#').map(|(_, service)| service)
    }
}

impl<S: Bos<str>> DidService<S> {
    /// Infallible unchecked constructor.
    ///
    /// # Safety
    ///
    /// The caller must ensure the DID service audience is valid.
    pub unsafe fn unchecked(value: S) -> Self {
        Self(value)
    }

    /// Convert to a `DidService` with a different backing type.
    pub fn convert<B: Bos<str> + From<S>>(self) -> DidService<B> {
        DidService(B::from(self.0))
    }

    /// Borrow as a `DidService<&str>`.
    pub fn borrow(&self) -> DidService<&str>
    where
        S: AsRef<str>,
    {
        // SAFETY: self is already validated.
        unsafe { DidService::unchecked(self.0.as_ref()) }
    }
}

impl<S: BosStr> DidService<S> {
    /// Fallible constructor that validates and wraps the input directly.
    pub fn new(s: S) -> Result<Self, AtStrError> {
        validate_did_service(s.as_ref())?;
        Ok(Self(s))
    }

    /// Infallible constructor. Panics on invalid DID service audiences.
    pub fn raw(s: S) -> Self {
        Self::new(s).expect("invalid DID service audience")
    }
}

impl<S: BosStr + FromStr> DidService<S> {
    /// Fallible constructor that validates and takes ownership.
    pub fn new_owned(value: impl AsRef<str>) -> Result<Self, AtStrError> {
        let value = value.as_ref();
        validate_did_service(value)?;
        let s = S::from_str(value).map_err(|_| {
            AtStrError::new(
                "did service audience",
                value.to_string(),
                StrParseKind::Conversion,
            )
        })?;
        Ok(Self(s))
    }

    /// Fallible constructor for static strings.
    pub fn new_static(value: &'static str) -> Result<Self, AtStrError> {
        validate_did_service(value)?;
        let s = S::from_str(value).map_err(|_| {
            AtStrError::new(
                "did service audience",
                value.to_string(),
                StrParseKind::Conversion,
            )
        })?;
        Ok(Self(s))
    }
}

impl<'de, S> Deserialize<'de> for DidService<S>
where
    S: BosStr + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = S::deserialize(deserializer)?;
        validate_did_service(s.as_ref()).map_err(serde::de::Error::custom)?;
        Ok(Self(s))
    }
}

impl<S> IntoStatic for DidService<S>
where
    S: Bos<str> + IntoStatic,
    S::Output: Bos<str>,
{
    type Output = DidService<S::Output>;

    fn into_static(self) -> Self::Output {
        DidService(self.0.into_static())
    }
}

impl<S: BosStr + FromStr> FromStr for DidService<S> {
    type Err = AtStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new_owned(s)
    }
}

impl<S: BosStr> fmt::Display for DidService<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<S: BosStr> fmt::Debug for DidService<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<S: BosStr> From<DidService<S>> for String {
    fn from(value: DidService<S>) -> Self {
        value.as_str().to_string()
    }
}

impl<S: BosStr> From<DidService<S>> for CowStr<'static> {
    fn from(value: DidService<S>) -> Self {
        CowStr::copy_from_str(value.as_str())
    }
}

impl From<String> for DidService {
    fn from(value: String) -> Self {
        Self::new_owned(value).unwrap()
    }
}

impl<'d> From<CowStr<'d>> for DidService<CowStr<'d>> {
    fn from(value: CowStr<'d>) -> Self {
        Self::new(value).unwrap()
    }
}

impl<S: BosStr> AsRef<str> for DidService<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: BosStr> Deref for DidService<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_with_service_id() {
        assert!(DidService::<&str>::new("did:web:example.com#bsky_appview").is_ok());
        assert!(DidService::<&str>::new("did:plc:abc123#atproto_labeler").is_ok());
    }

    #[test]
    fn valid_bare_did() {
        assert!(DidService::<&str>::new("did:web:example.com").is_ok());
    }

    #[test]
    fn splits_audience_and_service() {
        let value = DidService::<&str>::new("did:web:example.com#bsky_appview").unwrap();
        assert_eq!(value.audience().as_str(), "did:web:example.com");
        assert_eq!(value.service(), Some("bsky_appview"));

        let bare = DidService::<&str>::new("did:web:example.com").unwrap();
        assert_eq!(bare.audience().as_str(), "did:web:example.com");
        assert_eq!(bare.service(), None);
    }

    #[test]
    fn rejects_empty_fragment() {
        assert!(DidService::<&str>::new("did:web:example.com#").is_err());
    }

    #[test]
    fn rejects_invalid_service_chars() {
        for value in [
            "did:web:example.com#1bad",
            "did:web:example.com#-bad",
            "did:web:example.com#bad.service",
            "did:web:example.com#bad:service",
            "did:web:example.com#bad service",
            "did:web:example.com#bad#service",
        ] {
            assert!(DidService::<&str>::new(value).is_err(), "{value}");
        }
    }

    #[test]
    fn rejects_invalid_did_body() {
        assert!(DidService::<&str>::new("not-a-did#service").is_err());
    }

    #[test]
    fn enforces_max_length() {
        let service = "a".repeat(2049 - "did:web:example.com#".len());
        let value = format!("did:web:example.com#{service}");
        assert!(DidService::<&str>::new(&value).is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let value = DidService::new_static("did:web:example.com#bsky_appview").unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json, json!("did:web:example.com#bsky_appview"));
        let decoded: DidService = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn into_static_preserves_value() {
        let value = DidService::<CowStr<'_>>::new(CowStr::copy_from_str(
            "did:web:example.com#bsky_appview",
        ))
        .unwrap();
        let static_value: DidService<CowStr<'static>> = value.into_static();
        assert_eq!(static_value.as_str(), "did:web:example.com#bsky_appview");
    }

    #[test]
    fn from_str_owns_value() {
        let value: DidService = "did:web:example.com#bsky_appview".parse().unwrap();
        assert_eq!(value.as_str(), "did:web:example.com#bsky_appview");
    }
}
