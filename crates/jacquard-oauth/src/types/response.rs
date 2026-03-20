use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// The response from a Pushed Authorization Request (PAR) endpoint.
///
/// The returned `request_uri` is used in place of inline authorization parameters
/// when redirecting the user to the authorization server.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthParResponse {
    /// A short-lived URI representing the pushed authorization request.
    pub request_uri: SmolStr,
    /// Number of seconds until the `request_uri` expires.
    pub expires_in: Option<u32>,
}

/// The token type returned by the authorization server, indicating how to present the token.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum OAuthTokenType {
    /// Demonstration of Proof of Possession (DPoP) token (RFC 9449). Requires a DPoP proof header.
    DPoP,
    /// Standard Bearer token (RFC 6750). Sent as `Authorization: Bearer <token>`.
    Bearer,
}

impl OAuthTokenType {
    /// Returns the string representation used in HTTP `Authorization` headers.
    pub fn as_str(&self) -> &'static str {
        match self {
            OAuthTokenType::DPoP => "DPoP",
            OAuthTokenType::Bearer => "Bearer",
        }
    }
}

/// A successful token response from the authorization server (RFC 6749 §5.1).
/// <https://datatracker.ietf.org/doc/html/rfc6749#section-5.1>
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenResponse {
    /// The issued access token.
    pub access_token: SmolStr,
    /// The type of token, indicating the presentation scheme to use.
    pub token_type: OAuthTokenType,
    /// Lifetime of the access token in seconds from the time of issuance.
    pub expires_in: Option<i64>,
    /// A refresh token that can be used to obtain new access tokens.
    pub refresh_token: Option<SmolStr>,
    /// The scopes actually granted, if different from those requested.
    pub scope: Option<SmolStr>,
    // ATPROTO extension: add the sub claim to the token response to allow
    // clients to resolve the PDS url (audience) using the did resolution
    // mechanism.
    /// The subject (DID) the token was issued for; ATProto extension for PDS discovery.
    pub sub: Option<SmolStr>,
}
