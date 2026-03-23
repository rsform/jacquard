use jacquard_common::{IntoStatic, bos::{BosStr, DefaultStr}};
use jose_jwa::Algorithm;
use jose_jwk::Jwk;
use serde::{Deserialize, Serialize};

/// A JWS compact-serialization header, wrapping the registered header fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct Header<S: BosStr = DefaultStr> {
    /// The registered header parameters defined by the JWS specification.
    #[serde(flatten)]
    pub registered: RegisteredHeader<S>,
}

impl<S: BosStr> From<Header<S>> for super::super::jose::Header<S> {
    fn from(header: Header<S>) -> Self {
        super::super::jose::Header::Jws(header)
    }
}

/// Registered JWS header parameters as defined in RFC 7515 §4.1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct RegisteredHeader<S: BosStr = DefaultStr> {
    /// The cryptographic algorithm used to sign the JWS (e.g., `ES256`).
    pub alg: Algorithm,
    /// JWK Set URL: a URI pointing to a resource containing the public key(s) used to sign the JWS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jku: Option<S>,
    /// JSON Web Key: the public key used to verify the JWS, embedded directly in the header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwk: Option<Jwk>,
    /// Key ID: a hint indicating which key was used to sign the JWS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kid: Option<S>,
    /// X.509 URL: a URI pointing to a resource for the X.509 certificate used to sign the JWS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x5u: Option<S>,
    /// X.509 certificate chain: the certificate (and chain) corresponding to the key used to sign the JWS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x5c: Option<S>,
    /// X.509 certificate SHA-1 thumbprint: base64url-encoded SHA-1 digest of the DER-encoded certificate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x5t: Option<S>,
    /// X.509 certificate SHA-256 thumbprint: base64url-encoded SHA-256 digest of the DER-encoded certificate.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "x5t#S256")]
    pub x5ts256: Option<S>,
    /// Type: declares the media type of the complete JWS, used by applications to disambiguate among JOSe objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typ: Option<S>,
    /// Content type: declares the media type of the secured content (the payload).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cty: Option<S>,
}

impl<S: BosStr> From<Algorithm> for RegisteredHeader<S> {
    fn from(alg: Algorithm) -> Self {
        Self {
            alg,
            jku: None,
            jwk: None,
            kid: None,
            x5u: None,
            x5c: None,
            x5t: None,
            x5ts256: None,
            typ: None,
            cty: None,
        }
    }
}

impl<S: BosStr> From<RegisteredHeader<S>> for super::super::jose::Header<S> {
    fn from(registered: RegisteredHeader<S>) -> Self {
        super::super::jose::Header::Jws(Header { registered })
    }
}

impl<S> IntoStatic for RegisteredHeader<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = RegisteredHeader<S::Output>;
    fn into_static(self) -> Self::Output {
        RegisteredHeader {
            alg: self.alg,
            jku: self.jku.map(IntoStatic::into_static),
            jwk: self.jwk,
            kid: self.kid.map(IntoStatic::into_static),
            x5u: self.x5u.map(IntoStatic::into_static),
            x5c: self.x5c.map(IntoStatic::into_static),
            x5t: self.x5t.map(IntoStatic::into_static),
            x5ts256: self.x5ts256.map(IntoStatic::into_static),
            typ: self.typ.map(IntoStatic::into_static),
            cty: self.cty.map(IntoStatic::into_static),
        }
    }
}

impl<S> IntoStatic for Header<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = Header<S::Output>;
    fn into_static(self) -> Self::Output {
        Header {
            registered: self.registered.into_static(),
        }
    }
}
