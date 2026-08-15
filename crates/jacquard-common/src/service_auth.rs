//! Service authentication JWT parsing and verification for AT Protocol.
//!
//! Service auth is atproto's inter-service authentication mechanism. When a backend
//! service (feed generator, labeler, etc.) receives requests, the PDS signs a
//! short-lived JWT with the user's signing key and includes it as a Bearer token.
//!
//! # JWT Structure
//!
//! - Header: `alg` (ES256K for k256, ES256 for p256), `typ` ("JWT")
//! - Payload:
//!   - `iss`: user's DID (issuer)
//!   - `aud`: target service DID (audience)
//!   - `exp`: expiration unix timestamp
//!   - `iat`: issued at unix timestamp
//!   - `jti`: random nonce (128-bit hex) for replay protection
//!   - `lxm`: lexicon method NSID (method binding)
//! - Signature: signed with user's signing key from DID doc (ES256 or ES256K)

use crate::CowStr;
use crate::IntoStatic;
use crate::bos::{BosStr, DefaultStr};
use crate::types::string::{Did, DidService, Nsid};
use alloc::string::String;
use alloc::vec::Vec;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use signature::Verifier;
use smol_str::SmolStr;
use smol_str::format_smolstr;
use thiserror::Error;

#[cfg(feature = "crypto-p256")]
use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256VerifyingKey};

#[cfg(feature = "crypto-k256")]
use k256::ecdsa::{Signature as K256Signature, VerifyingKey as K256VerifyingKey};

/// Errors that can occur during JWT parsing and verification.
#[derive(Debug, Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ServiceAuthError {
    /// JWT format is invalid (not three base64-encoded parts separated by dots)
    #[error("malformed JWT: {0}")]
    MalformedToken(CowStr<'static>),

    /// Base64 decoding failed
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    /// JSON parsing failed
    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Signature verification failed
    #[error("invalid signature")]
    InvalidSignature,

    /// Unsupported algorithm
    #[error("unsupported algorithm: {alg}")]
    UnsupportedAlgorithm {
        /// Algorithm name from JWT header
        alg: SmolStr,
    },

    /// Token has expired
    #[error("token expired at {exp} (current time: {now})")]
    Expired {
        /// Expiration timestamp from token
        exp: i64,
        /// Current timestamp
        now: i64,
    },

    /// Audience mismatch.
    #[error("audience mismatch: expected {expected}, got {actual}")]
    AudienceMismatch {
        /// Expected audience DID.
        expected: Did,
        /// Actual audience DID service in token.
        actual: DidService,
    },

    /// Service id mismatch.
    #[error("service id mismatch: allowed {allowed:?}, got {actual:?}")]
    ServiceIdMismatch {
        /// Allowed service ids.
        allowed: Vec<SmolStr>,
        /// Actual service id in token.
        actual: Option<SmolStr>,
    },

    /// Method mismatch (lxm field)
    #[error("method mismatch: expected {expected}, got {actual:?}")]
    MethodMismatch {
        /// Expected method NSID
        expected: Nsid,
        /// Actual method NSID in token (if any)
        actual: Option<Nsid>,
    },

    /// Missing required field
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// Crypto error
    #[error("crypto error: {0}")]
    Crypto(CowStr<'static>),
}

/// JWT header for service auth tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtHeader<S: BosStr = DefaultStr> {
    /// Algorithm used for signing.
    pub alg: S,
    /// Type (always "JWT").
    pub typ: S,
}

impl<S> IntoStatic for JwtHeader<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = JwtHeader<S::Output>;

    fn into_static(self) -> Self::Output {
        JwtHeader {
            alg: self.alg.into_static(),
            typ: self.typ.into_static(),
        }
    }
}

/// Service authentication claims.
///
/// These are the payload fields in a service auth JWT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAuthClaims<S: BosStr = DefaultStr> {
    /// Issuer (user's DID).
    pub iss: Did<S>,

    /// Audience (target service DID with optional service-id fragment).
    pub aud: DidService<S>,

    /// Expiration time (unix timestamp).
    pub exp: i64,

    /// Issued at (unix timestamp).
    pub iat: i64,

    /// JWT ID (nonce for replay protection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<S>,

    /// Lexicon method NSID (method binding).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lxm: Option<Nsid<S>>,
}

impl<S> IntoStatic for ServiceAuthClaims<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ServiceAuthClaims<S::Output>;

    fn into_static(self) -> Self::Output {
        ServiceAuthClaims {
            iss: self.iss.into_static(),
            aud: self.aud.into_static(),
            exp: self.exp,
            iat: self.iat,
            jti: self.jti.map(|j| j.into_static()),
            lxm: self.lxm.map(|l| l.into_static()),
        }
    }
}

impl<S: BosStr> ServiceAuthClaims<S> {
    /// Validate the claims against expected values.
    ///
    /// Checks:
    /// - The fragmentless audience matches the expected DID.
    /// - A present service-id fragment is in the allowed service list when configured.
    /// - The token is not expired.
    pub fn validate<B, Svc>(
        &self,
        expected_aud: &Did<B>,
        allowed_services: &[Svc],
    ) -> Result<(), ServiceAuthError>
    where
        B: BosStr,
        Svc: AsRef<str>,
    {
        if self.aud.audience().as_str() != expected_aud.as_str() {
            return Err(ServiceAuthError::AudienceMismatch {
                expected: expected_aud.borrow().into_static(),
                actual: self.aud.borrow().into_static(),
            });
        }

        if !allowed_services.is_empty() {
            if let Some(service) = self.aud.service() {
                if !allowed_services
                    .iter()
                    .any(|allowed| allowed.as_ref() == service)
                {
                    return Err(ServiceAuthError::ServiceIdMismatch {
                        allowed: allowed_services
                            .iter()
                            .map(|allowed| SmolStr::new(allowed.as_ref()))
                            .collect(),
                        actual: Some(SmolStr::new(service)),
                    });
                }
            }
        }

        if self.is_expired() {
            let now = chrono::Utc::now().timestamp();
            return Err(ServiceAuthError::Expired { exp: self.exp, now });
        }

        Ok(())
    }

    /// Check if the token has expired.
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.exp <= now
    }

    /// Check if the method (lxm) matches the expected NSID.
    pub fn check_method(&self, nsid: &Nsid<CowStr<'_>>) -> bool {
        self.lxm
            .as_ref()
            .map(|lxm| lxm.as_str() == nsid.as_str())
            .unwrap_or(false)
    }

    /// Require that the method (lxm) matches the expected NSID.
    pub fn require_method(&self, nsid: &Nsid<CowStr<'_>>) -> Result<(), ServiceAuthError> {
        if !self.check_method(nsid) {
            return Err(ServiceAuthError::MethodMismatch {
                expected: nsid.borrow().into_static(),
                actual: self.lxm.as_ref().map(|lxm| lxm.borrow().into_static()),
            });
        }
        Ok(())
    }
}

/// Parsed JWT components with service-auth claims.
///
/// A convenience alias over [`ParsedJwtWithClaims`]; this struct owns decoded
/// and parsed JWT data. `signing_input` stores the original `header.payload`
/// bytes used for signature verification.
pub type ParsedJwt<S = DefaultStr> = ParsedJwtWithClaims<ServiceAuthClaims<S>>;

/// A parsed JWT with a caller-defined claims shape, for token families whose
/// claims do not fit [`ServiceAuthClaims`] (space credentials, delegation
/// tokens, client attestations). Signature verification goes through
/// [`verify_signature`], which only needs the header, signing input, and
/// signature — all retained here.
pub struct ParsedJwtWithClaims<C> {
    /// Parsed JWT header.
    header: JwtHeader,
    /// Parsed caller-defined claims.
    claims: C,
    /// Original `header.payload` signing input.
    signing_input: String,
    /// Decoded signature bytes.
    signature: Vec<u8>,
}

impl<C> ParsedJwtWithClaims<C> {
    /// Get the signing input (header.payload) for signature verification.
    pub fn signing_input(&self) -> &[u8] {
        self.signing_input.as_bytes()
    }

    /// Get a reference to the header.
    pub fn header(&self) -> &JwtHeader {
        &self.header
    }

    /// Get a reference to the custom claims.
    pub fn claims(&self) -> &C {
        &self.claims
    }

    /// Get the decoded signature bytes.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// Consume the parsed token, returning the custom claims.
    pub fn into_claims(self) -> C {
        self.claims
    }
}

/// Parse a JWT token into its components without verifying the signature.
///
/// This extracts and decodes all JWT components. The header and claims are parsed
/// into their default owned backing types.
pub fn parse_jwt(token: &str) -> Result<ParsedJwt, ServiceAuthError> {
    parse_jwt_with_claims::<ServiceAuthClaims>(token)
}

/// Parse a JWT token with a custom claims shape, without verifying the signature.
///
/// Space credentials, delegation tokens, and client attestations share the JWT
/// envelope but carry claim sets that do not fit [`ServiceAuthClaims`]; callers
/// verify the signature with [`verify_signature`] against their own parsed form.
pub fn parse_jwt_with_claims<C>(token: &str) -> Result<ParsedJwtWithClaims<C>, ServiceAuthError>
where
    C: serde::de::DeserializeOwned,
{
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(ServiceAuthError::MalformedToken(CowStr::new_static(
            "JWT must have exactly 3 parts separated by dots",
        )));
    }

    let header_b64 = parts[0];
    let payload_b64 = parts[1];
    let signature_b64 = parts[2];

    let header_buf = URL_SAFE_NO_PAD.decode(header_b64)?;
    let payload_buf = URL_SAFE_NO_PAD.decode(payload_b64)?;
    let signature = URL_SAFE_NO_PAD.decode(signature_b64)?;
    let header: JwtHeader = serde_json::from_slice(&header_buf)?;
    let claims: C = serde_json::from_slice(&payload_buf)?;
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    Ok(ParsedJwtWithClaims {
        header,
        claims,
        signing_input,
        signature,
    })
}

/// Public key types for signature verification.
#[derive(Debug, Clone)]
pub enum PublicKey {
    /// P-256 (ES256) public key
    #[cfg(feature = "crypto-p256")]
    P256(P256VerifyingKey),

    /// secp256k1 (ES256K) public key
    #[cfg(feature = "crypto-k256")]
    K256(K256VerifyingKey),
}

impl PublicKey {
    /// Create a P-256 public key from compressed or uncompressed bytes.
    #[cfg(feature = "crypto-p256")]
    pub fn from_p256_bytes(bytes: &[u8]) -> Result<Self, ServiceAuthError> {
        let key = P256VerifyingKey::from_sec1_bytes(bytes).map_err(|e| {
            ServiceAuthError::Crypto(CowStr::Owned(format_smolstr!("invalid P-256 key: {}", e)))
        })?;
        Ok(PublicKey::P256(key))
    }

    /// Create a secp256k1 public key from compressed or uncompressed bytes.
    #[cfg(feature = "crypto-k256")]
    pub fn from_k256_bytes(bytes: &[u8]) -> Result<Self, ServiceAuthError> {
        let key = K256VerifyingKey::from_sec1_bytes(bytes).map_err(|e| {
            ServiceAuthError::Crypto(CowStr::Owned(format_smolstr!("invalid K-256 key: {}", e)))
        })?;
        Ok(PublicKey::K256(key))
    }
}

/// Verify a JWT signature using the provided public key.
///
/// The algorithm is determined by the JWT header and must match the public key type.
pub fn verify_signature<C>(
    parsed: &ParsedJwtWithClaims<C>,
    public_key: &PublicKey,
) -> Result<(), ServiceAuthError> {
    let alg = parsed.header().alg.as_str();
    let signing_input = parsed.signing_input();
    let signature = parsed.signature();

    match (alg, public_key) {
        #[cfg(feature = "crypto-p256")]
        ("ES256", PublicKey::P256(key)) => {
            let sig = P256Signature::from_slice(signature).map_err(|e| {
                ServiceAuthError::Crypto(CowStr::Owned(format_smolstr!(
                    "invalid ES256 signature: {}",
                    e
                )))
            })?;
            key.verify(signing_input, &sig)
                .map_err(|_| ServiceAuthError::InvalidSignature)?;
            Ok(())
        }

        #[cfg(feature = "crypto-k256")]
        ("ES256K", PublicKey::K256(key)) => {
            let sig = K256Signature::from_slice(signature).map_err(|e| {
                ServiceAuthError::Crypto(CowStr::Owned(format_smolstr!(
                    "invalid ES256K signature: {}",
                    e
                )))
            })?;
            key.verify(signing_input, &sig)
                .map_err(|_| ServiceAuthError::InvalidSignature)?;
            Ok(())
        }

        _ => Err(ServiceAuthError::UnsupportedAlgorithm {
            alg: SmolStr::new(alg),
        }),
    }
}

/// Parse and verify a service auth JWT in one step, returning owned claims.
///
/// This is a convenience function that combines parsing and signature verification.
pub fn verify_service_jwt(
    token: &str,
    public_key: &PublicKey,
) -> Result<ServiceAuthClaims, ServiceAuthError> {
    let parsed = parse_jwt(token)?;
    verify_signature(&parsed, public_key)?;
    Ok(parsed.into_claims())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jwt_invalid_format() {
        let result = parse_jwt("not.a.valid.jwt.with.too.many.parts");
        assert!(matches!(result, Err(ServiceAuthError::MalformedToken(_))));
    }

    #[test]
    fn test_claims_expiration() {
        let now = chrono::Utc::now().timestamp();
        let expired_claims: ServiceAuthClaims = ServiceAuthClaims {
            iss: Did::new_static("did:plc:test").unwrap(),
            aud: DidService::new_static("did:web:example.com").unwrap(),
            exp: now - 100,
            iat: now - 200,
            jti: None,
            lxm: None,
        };

        assert!(expired_claims.is_expired());

        let valid_claims: ServiceAuthClaims = ServiceAuthClaims {
            iss: Did::new_static("did:plc:test").unwrap(),
            aud: DidService::new_static("did:web:example.com").unwrap(),
            exp: now + 100,
            iat: now,
            jti: None,
            lxm: None,
        };

        assert!(!valid_claims.is_expired());
    }

    fn claims_with_aud(aud: &str) -> ServiceAuthClaims {
        ServiceAuthClaims {
            iss: Did::new_static("did:plc:test").unwrap(),
            aud: DidService::new_owned(aud).unwrap(),
            exp: chrono::Utc::now().timestamp() + 100,
            iat: chrono::Utc::now().timestamp(),
            jti: None,
            lxm: None,
        }
    }

    #[test]
    fn test_audience_validation() {
        let expected_aud = Did::new("did:web:example.com").unwrap();
        assert!(
            claims_with_aud("did:web:example.com")
                .validate(&expected_aud, &[] as &[&str])
                .is_ok()
        );
        assert!(
            claims_with_aud("did:web:example.com#bsky_appview")
                .validate(&expected_aud, &[] as &[&str])
                .is_ok()
        );
        assert!(
            claims_with_aud("did:web:example.com")
                .validate(&expected_aud, &["bsky_appview"])
                .is_ok()
        );
        assert!(
            claims_with_aud("did:web:example.com#bsky_appview")
                .validate(&expected_aud, &["bsky_appview"])
                .is_ok()
        );
        assert!(matches!(
            claims_with_aud("did:web:example.com#other").validate(&expected_aud, &["bsky_appview"]),
            Err(ServiceAuthError::ServiceIdMismatch { .. })
        ));

        let wrong_aud = Did::new("did:web:wrong.com").unwrap();
        assert!(matches!(
            claims_with_aud("did:web:example.com#bsky_appview")
                .validate(&wrong_aud, &["bsky_appview"]),
            Err(ServiceAuthError::AudienceMismatch { .. })
        ));
    }

    #[test]
    fn test_method_check() {
        let claims: ServiceAuthClaims = ServiceAuthClaims {
            iss: Did::new_static("did:plc:test").unwrap(),
            aud: DidService::new_static("did:web:example.com").unwrap(),
            exp: chrono::Utc::now().timestamp() + 100,
            iat: chrono::Utc::now().timestamp(),
            jti: None,
            lxm: Some(Nsid::new_static("app.bsky.feed.getFeedSkeleton").unwrap()),
        };

        let expected = Nsid::new_static("app.bsky.feed.getFeedSkeleton").unwrap();
        assert!(claims.check_method(&expected));

        let wrong = Nsid::new_static("app.bsky.feed.getTimeline".into()).unwrap();
        assert!(!claims.check_method(&wrong));
    }
}
