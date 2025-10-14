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
use crate::types::string::{Did, Nsid};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ouroboros::self_referencing;
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

    /// Audience mismatch
    #[error("audience mismatch: expected {expected}, got {actual}")]
    AudienceMismatch {
        /// Expected audience DID
        expected: Did<'static>,
        /// Actual audience DID in token
        actual: Did<'static>,
    },

    /// Method mismatch (lxm field)
    #[error("method mismatch: expected {expected}, got {actual:?}")]
    MethodMismatch {
        /// Expected method NSID
        expected: Nsid<'static>,
        /// Actual method NSID in token (if any)
        actual: Option<Nsid<'static>>,
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
pub struct JwtHeader<'a> {
    /// Algorithm used for signing
    #[serde(borrow)]
    pub alg: CowStr<'a>,
    /// Type (always "JWT")
    #[serde(borrow)]
    pub typ: CowStr<'a>,
}

impl IntoStatic for JwtHeader<'_> {
    type Output = JwtHeader<'static>;

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
pub struct ServiceAuthClaims<'a> {
    /// Issuer (user's DID)
    #[serde(borrow)]
    pub iss: Did<'a>,

    /// Audience (target service DID)
    #[serde(borrow)]
    pub aud: Did<'a>,

    /// Expiration time (unix timestamp)
    pub exp: i64,

    /// Issued at (unix timestamp)
    pub iat: i64,

    /// JWT ID (nonce for replay protection)
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub jti: Option<CowStr<'a>>,

    /// Lexicon method NSID (method binding)
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub lxm: Option<Nsid<'a>>,
}

impl<'a> IntoStatic for ServiceAuthClaims<'a> {
    type Output = ServiceAuthClaims<'static>;

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

impl<'a> ServiceAuthClaims<'a> {
    /// Validate the claims against expected values.
    ///
    /// Checks:
    /// - Audience matches expected DID
    /// - Token is not expired
    pub fn validate(&self, expected_aud: &Did) -> Result<(), ServiceAuthError> {
        // Check audience
        if self.aud.as_str() != expected_aud.as_str() {
            return Err(ServiceAuthError::AudienceMismatch {
                expected: expected_aud.clone().into_static(),
                actual: self.aud.clone().into_static(),
            });
        }

        // Check expiration
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
    pub fn check_method(&self, nsid: &Nsid) -> bool {
        self.lxm
            .as_ref()
            .map(|lxm| lxm.as_str() == nsid.as_str())
            .unwrap_or(false)
    }

    /// Require that the method (lxm) matches the expected NSID.
    pub fn require_method(&self, nsid: &Nsid) -> Result<(), ServiceAuthError> {
        if !self.check_method(nsid) {
            return Err(ServiceAuthError::MethodMismatch {
                expected: nsid.clone().into_static(),
                actual: self.lxm.as_ref().map(|l| l.clone().into_static()),
            });
        }
        Ok(())
    }
}

/// Parsed JWT components.
///
/// This struct owns the decoded buffers and parsed components using ouroboros
/// self-referencing. The header and claims borrow from their respective buffers.
#[self_referencing]
pub struct ParsedJwt {
    /// Decoded header buffer (owned)
    header_buf: Vec<u8>,
    /// Decoded payload buffer (owned)
    payload_buf: Vec<u8>,
    /// Original token string for signing_input
    token: String,
    /// Signature bytes
    signature: Vec<u8>,
    /// Parsed header borrowing from header_buf
    #[borrows(header_buf)]
    #[covariant]
    header: JwtHeader<'this>,
    /// Parsed claims borrowing from payload_buf
    #[borrows(payload_buf)]
    #[covariant]
    claims: ServiceAuthClaims<'this>,
}

impl ParsedJwt {
    /// Get the signing input (header.payload) for signature verification.
    pub fn signing_input(&self) -> &[u8] {
        self.with_token(|token| {
            let dot_pos = token.find('.').unwrap();
            let second_dot_pos = token[dot_pos + 1..].find('.').unwrap() + dot_pos + 1;
            token[..second_dot_pos].as_bytes()
        })
    }

    /// Get a reference to the header.
    pub fn header(&self) -> &JwtHeader<'_> {
        self.borrow_header()
    }

    /// Get a reference to the claims.
    pub fn claims(&self) -> &ServiceAuthClaims<'_> {
        self.borrow_claims()
    }

    /// Get a reference to the signature.
    pub fn signature(&self) -> &[u8] {
        self.borrow_signature()
    }

    /// Get owned header with 'static lifetime.
    pub fn into_header(self) -> JwtHeader<'static> {
        self.with_header(|header| header.clone().into_static())
    }

    /// Get owned claims with 'static lifetime.
    pub fn into_claims(self) -> ServiceAuthClaims<'static> {
        self.with_claims(|claims| claims.clone().into_static())
    }
}

/// Parse a JWT token into its components without verifying the signature.
///
/// This extracts and decodes all JWT components. The header and claims are parsed
/// and borrow from their respective owned buffers using ouroboros self-referencing.
pub fn parse_jwt(token: &str) -> Result<ParsedJwt, ServiceAuthError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(ServiceAuthError::MalformedToken(CowStr::new_static(
            "JWT must have exactly 3 parts separated by dots",
        )));
    }

    let header_b64 = parts[0];
    let payload_b64 = parts[1];
    let signature_b64 = parts[2];

    // Decode all components
    let header_buf = URL_SAFE_NO_PAD.decode(header_b64)?;
    let payload_buf = URL_SAFE_NO_PAD.decode(payload_b64)?;
    let signature = URL_SAFE_NO_PAD.decode(signature_b64)?;

    // Validate that buffers contain valid JSON for their types
    // We parse once here to validate, then again in the builder (unavoidable with ouroboros)
    let _header: JwtHeader = serde_json::from_slice(&header_buf)?;
    let _claims: ServiceAuthClaims = serde_json::from_slice(&payload_buf)?;

    Ok(ParsedJwtBuilder {
        header_buf,
        payload_buf,
        token: token.to_string(),
        signature,
        header_builder: |buf| {
            // Safe: we validated this succeeds above
            serde_json::from_slice(buf).expect("header was validated")
        },
        claims_builder: |buf| {
            // Safe: we validated this succeeds above
            serde_json::from_slice(buf).expect("claims were validated")
        },
    }
    .build())
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
pub fn verify_signature(
    parsed: &ParsedJwt,
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
) -> Result<ServiceAuthClaims<'static>, ServiceAuthError> {
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
        let expired_claims = ServiceAuthClaims {
            iss: Did::new("did:plc:test").unwrap(),
            aud: Did::new("did:web:example.com").unwrap(),
            exp: now - 100,
            iat: now - 200,
            jti: None,
            lxm: None,
        };

        assert!(expired_claims.is_expired());

        let valid_claims = ServiceAuthClaims {
            iss: Did::new("did:plc:test").unwrap(),
            aud: Did::new("did:web:example.com").unwrap(),
            exp: now + 100,
            iat: now,
            jti: None,
            lxm: None,
        };

        assert!(!valid_claims.is_expired());
    }

    #[test]
    fn test_audience_validation() {
        let now = chrono::Utc::now().timestamp();
        let claims = ServiceAuthClaims {
            iss: Did::new("did:plc:test").unwrap(),
            aud: Did::new("did:web:example.com").unwrap(),
            exp: now + 100,
            iat: now,
            jti: None,
            lxm: None,
        };

        let expected_aud = Did::new("did:web:example.com").unwrap();
        assert!(claims.validate(&expected_aud).is_ok());

        let wrong_aud = Did::new("did:web:wrong.com").unwrap();
        assert!(matches!(
            claims.validate(&wrong_aud),
            Err(ServiceAuthError::AudienceMismatch { .. })
        ));
    }

    #[test]
    fn test_method_check() {
        let claims = ServiceAuthClaims {
            iss: Did::new("did:plc:test").unwrap(),
            aud: Did::new("did:web:example.com").unwrap(),
            exp: chrono::Utc::now().timestamp() + 100,
            iat: chrono::Utc::now().timestamp(),
            jti: None,
            lxm: Some(Nsid::new("app.bsky.feed.getFeedSkeleton").unwrap()),
        };

        let expected = Nsid::new("app.bsky.feed.getFeedSkeleton").unwrap();
        assert!(claims.check_method(&expected));

        let wrong = Nsid::new("app.bsky.feed.getTimeline").unwrap();
        assert!(!claims.check_method(&wrong));
    }
}
