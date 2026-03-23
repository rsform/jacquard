use jacquard_common::{
    IntoStatic,
    bos::{BosStr, DefaultStr},
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Full JWT claims payload, combining registered and public (DPoP-specific) claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct Claims<S: BosStr = DefaultStr> {
    /// Standard registered JWT claims (iss, sub, aud, exp, etc.).
    #[serde(flatten)]
    pub registered: RegisteredClaims<S>,
    /// Public claims used in DPoP proofs (htm, htu, ath, nonce).
    #[serde(flatten)]
    pub public: PublicClaims<S>,
}

/// Manual `Default` impl to avoid a spurious `S: Default` bound from the derive macro.
///
/// All `S`-typed fields are wrapped in `Option<S>`, which is `Default` regardless of `S`.
impl<S: BosStr> Default for Claims<S> {
    fn default() -> Self {
        Self {
            registered: RegisteredClaims::default(),
            public: PublicClaims::default(),
        }
    }
}

/// Standard registered JWT claims as defined in RFC 7519 §4.1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct RegisteredClaims<S: BosStr = DefaultStr> {
    /// Issuer: identifies the principal that issued the JWT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<S>,
    /// Subject: identifies the principal that is the subject of the JWT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<S>,
    /// Audience: recipients that the JWT is intended for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<RegisteredClaimsAud<S>>,
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
    pub jti: Option<SmolStr>,
}

/// Manual `Default` impl to avoid a spurious `S: Default` bound from the derive macro.
///
/// All `S`-typed fields are wrapped in `Option<S>`, which is `Default` regardless of `S`.
impl<S: BosStr> Default for RegisteredClaims<S> {
    fn default() -> Self {
        Self {
            iss: None,
            sub: None,
            aud: None,
            exp: None,
            nbf: None,
            iat: None,
            jti: None,
        }
    }
}

/// Public claims used in DPoP proof JWTs (RFC 9449).
///
/// These claims bind the DPoP proof to a specific HTTP request, preventing
/// the proof from being replayed against a different endpoint or method.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct PublicClaims<S: BosStr = DefaultStr> {
    /// HTTP method of the request the DPoP proof is bound to (e.g., `"POST"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub htm: Option<S>,
    /// HTTP target URI of the request the DPoP proof is bound to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub htu: Option<S>,
    /// Access token hash: base64url-encoded SHA-256 of the access token, binding the proof to a specific token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ath: Option<S>,
    /// Server-provided nonce, included to prevent replay attacks when required by the authorization server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<S>,
}

/// Manual `Default` impl to avoid a spurious `S: Default` bound from the derive macro.
///
/// All `S`-typed fields are wrapped in `Option<S>`, which is `Default` regardless of `S`.
impl<S: BosStr> Default for PublicClaims<S> {
    fn default() -> Self {
        Self {
            htm: None,
            htu: None,
            ath: None,
            nonce: None,
        }
    }
}

impl<S: BosStr> From<RegisteredClaims<S>> for Claims<S> {
    fn from(registered: RegisteredClaims<S>) -> Self {
        Self {
            registered,
            public: PublicClaims::default(),
        }
    }
}

/// The `aud` (audience) claim, which may be a single string or a list of strings per RFC 7519.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub enum RegisteredClaimsAud<S: BosStr = DefaultStr> {
    /// A single audience identifier.
    Single(S),
    /// Multiple audience identifiers.
    Multiple(Vec<S>),
}

impl<S> IntoStatic for RegisteredClaims<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = RegisteredClaims<S::Output>;
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

impl<S> IntoStatic for PublicClaims<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = PublicClaims<S::Output>;
    fn into_static(self) -> Self::Output {
        PublicClaims {
            htm: self.htm.map(IntoStatic::into_static),
            htu: self.htu.map(IntoStatic::into_static),
            ath: self.ath.map(IntoStatic::into_static),
            nonce: self.nonce.map(IntoStatic::into_static),
        }
    }
}

impl<S> IntoStatic for Claims<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = Claims<S::Output>;
    fn into_static(self) -> Self::Output {
        Claims {
            registered: self.registered.into_static(),
            public: self.public.into_static(),
        }
    }
}

impl<S> IntoStatic for RegisteredClaimsAud<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = RegisteredClaimsAud<S::Output>;
    fn into_static(self) -> Self::Output {
        match self {
            RegisteredClaimsAud::Single(s) => RegisteredClaimsAud::Single(s.into_static()),
            RegisteredClaimsAud::Multiple(v) => {
                RegisteredClaimsAud::Multiple(v.into_iter().map(IntoStatic::into_static).collect())
            }
        }
    }
}
