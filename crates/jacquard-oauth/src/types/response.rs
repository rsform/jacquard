use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthParResponse<'r> {
    #[serde(borrow)]
    pub request_uri: CowStr<'r>,
    pub expires_in: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum OAuthTokenType {
    DPoP,
    Bearer,
}

// https://datatracker.ietf.org/doc/html/rfc6749#section-5.1
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenResponse<'r> {
    #[serde(borrow)]
    pub access_token: CowStr<'r>,
    pub token_type: OAuthTokenType,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<CowStr<'r>>,
    pub scope: Option<CowStr<'r>>,
    // ATPROTO extension: add the sub claim to the token response to allow
    // clients to resolve the PDS url (audience) using the did resolution
    // mechanism.
    pub sub: Option<CowStr<'r>>,
}

impl IntoStatic for OAuthTokenResponse<'_> {
    type Output = OAuthTokenResponse<'static>;

    fn into_static(self) -> Self::Output {
        OAuthTokenResponse {
            access_token: self.access_token.into_static(),
            token_type: self.token_type,
            expires_in: self.expires_in,
            refresh_token: self.refresh_token.map(|s| s.into_static()),
            scope: self.scope.map(|s| s.into_static()),
            sub: self.sub.map(|s| s.into_static()),
        }
    }
}

impl IntoStatic for OAuthParResponse<'_> {
    type Output = OAuthParResponse<'static>;

    fn into_static(self) -> Self::Output {
        OAuthParResponse {
            request_uri: self.request_uri.into_static(),
            expires_in: self.expires_in,
        }
    }
}
