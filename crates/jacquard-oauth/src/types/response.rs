use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthParResponse {
    pub request_uri: SmolStr,
    pub expires_in: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum OAuthTokenType {
    DPoP,
    Bearer,
}

impl OAuthTokenType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OAuthTokenType::DPoP => "DPoP",
            OAuthTokenType::Bearer => "Bearer",
        }
    }
}

// https://datatracker.ietf.org/doc/html/rfc6749#section-5.1
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenResponse {
    pub access_token: SmolStr,
    pub token_type: OAuthTokenType,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<SmolStr>,
    pub scope: Option<SmolStr>,
    // ATPROTO extension: add the sub claim to the token response to allow
    // clients to resolve the PDS url (audience) using the did resolution
    // mechanism.
    pub sub: Option<SmolStr>,
}
