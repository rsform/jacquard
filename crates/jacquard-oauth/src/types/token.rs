use super::response::OAuthTokenType;
use jacquard_common::types::string::{Datetime, Did};
use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

/// A complete set of OAuth tokens and associated claims for an authenticated session.
///
/// Combines the token response with resolved identity claims to give the client
/// everything it needs to make authorized requests. This is stored in the session
/// and refreshed transparently by `OAuthSession`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TokenSet<'s> {
    /// The issuer URL of the authorization server that issued these tokens.
    #[serde(borrow)]
    pub iss: CowStr<'s>,
    /// The subject DID identifying the authenticated user.
    pub sub: Did<'s>,
    /// The audience (resource server URL or DID) the tokens are intended for.
    pub aud: CowStr<'s>,
    /// The scopes granted by the authorization server.
    pub scope: Option<CowStr<'s>>,

    /// A refresh token that can be exchanged for new access tokens.
    pub refresh_token: Option<CowStr<'s>>,
    /// The current access token to include in API requests.
    pub access_token: CowStr<'s>,
    /// Whether the access token must be presented as a DPoP or Bearer token.
    pub token_type: OAuthTokenType,

    /// The point in time at which the access token expires.
    pub expires_at: Option<Datetime>,
}

impl IntoStatic for TokenSet<'_> {
    type Output = TokenSet<'static>;

    fn into_static(self) -> Self::Output {
        TokenSet {
            iss: self.iss.into_static(),
            sub: self.sub.into_static(),
            aud: self.aud.into_static(),
            scope: self.scope.map(|s| s.into_static()),
            refresh_token: self.refresh_token.map(|s| s.into_static()),
            access_token: self.access_token.into_static(),
            token_type: self.token_type,
            expires_at: self.expires_at.map(|s| s.into_static()),
        }
    }
}
