use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

/// Full JWT claims payload, combining registered and public (DPoP-specific) claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Claims<'a> {
    /// Standard registered JWT claims (iss, sub, aud, exp, etc.).
    #[serde(flatten)]
    pub registered: RegisteredClaims<'a>,
    /// Public claims used in DPoP proofs (htm, htu, ath, nonce).
    #[serde(flatten)]
    #[serde(borrow)]
    pub public: PublicClaims<'a>,
}

/// Standard registered JWT claims as defined in RFC 7519 §4.1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]

pub struct RegisteredClaims<'a> {
    /// Issuer: identifies the principal that issued the JWT.
    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<CowStr<'a>>,
    /// Subject: identifies the principal that is the subject of the JWT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<CowStr<'a>>,
    /// Audience: recipients that the JWT is intended for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<RegisteredClaimsAud<'a>>,
    /// Expiration time (Unix timestamp): the JWT must not be accepted on or after this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    /// Not before (Unix timestamp): the JWT must not be accepted before this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// Issued at (Unix timestamp): identifies when the JWT was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    /// JWT ID: unique identifier for the token, used to prevent replay attacks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<CowStr<'a>>,
}

/// Public claims used in DPoP proof JWTs (RFC 9449).
///
/// These claims bind the DPoP proof to a specific HTTP request, preventing
/// the proof from being replayed against a different endpoint or method.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]

pub struct PublicClaims<'a> {
    /// HTTP method of the request the DPoP proof is bound to (e.g., `"POST"`).
    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub htm: Option<CowStr<'a>>,
    /// HTTP target URI of the request the DPoP proof is bound to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub htu: Option<CowStr<'a>>,
    /// Access token hash: base64url-encoded SHA-256 of the access token, binding the proof to a specific token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ath: Option<CowStr<'a>>,
    /// Server-provided nonce, included to prevent replay attacks when required by the authorization server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<CowStr<'a>>,
}

impl<'a> From<RegisteredClaims<'a>> for Claims<'a> {
    fn from(registered: RegisteredClaims<'a>) -> Self {
        Self {
            registered,
            public: PublicClaims::default(),
        }
    }
}

/// The `aud` (audience) claim, which may be a single string or a list of strings per RFC 7519.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegisteredClaimsAud<'a> {
    /// A single audience identifier.
    #[serde(borrow)]
    Single(CowStr<'a>),
    /// Multiple audience identifiers.
    Multiple(Vec<CowStr<'a>>),
}

impl IntoStatic for RegisteredClaims<'_> {
    type Output = RegisteredClaims<'static>;
    fn into_static(self) -> Self::Output {
        RegisteredClaims {
            iss: self.iss.map(IntoStatic::into_static),
            sub: self.sub.map(IntoStatic::into_static),
            aud: self.aud.map(IntoStatic::into_static),
            exp: self.exp,
            nbf: self.nbf,
            iat: self.iat,
            jti: self.jti.map(IntoStatic::into_static),
        }
    }
}

impl IntoStatic for PublicClaims<'_> {
    type Output = PublicClaims<'static>;
    fn into_static(self) -> Self::Output {
        PublicClaims {
            htm: self.htm.map(IntoStatic::into_static),
            htu: self.htu.map(IntoStatic::into_static),
            ath: self.ath.map(IntoStatic::into_static),
            nonce: self.nonce.map(IntoStatic::into_static),
        }
    }
}

impl IntoStatic for RegisteredClaimsAud<'_> {
    type Output = RegisteredClaimsAud<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            RegisteredClaimsAud::Single(s) => RegisteredClaimsAud::Single(s.into_static()),
            RegisteredClaimsAud::Multiple(v) => {
                RegisteredClaimsAud::Multiple(v.into_iter().map(IntoStatic::into_static).collect())
            }
        }
    }
}
