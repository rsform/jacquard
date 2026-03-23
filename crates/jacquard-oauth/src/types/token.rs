use super::response::OAuthTokenType;
use jacquard_common::IntoStatic;
use jacquard_common::bos::{BosStr, DefaultStr};
use jacquard_common::types::string::{Datetime, Did};
use serde::{Deserialize, Serialize};

/// A complete set of OAuth tokens and associated claims for an authenticated session.
///
/// Combines the token response with resolved identity claims to give the client
/// everything it needs to make authorized requests. This is stored in the session
/// and refreshed transparently by `OAuthSession`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct TokenSet<S: BosStr = DefaultStr> {
    /// The issuer URL of the authorization server that issued these tokens.
    pub iss: S,
    /// The subject DID identifying the authenticated user.
    pub sub: Did<S>,
    /// The audience (resource server URL or DID) the tokens are intended for.
    pub aud: S,
    /// The scopes granted by the authorization server.
    pub scope: Option<S>,

    /// A refresh token that can be exchanged for new access tokens.
    pub refresh_token: Option<S>,
    /// The current access token to include in API requests.
    pub access_token: S,
    /// Whether the access token must be presented as a DPoP or Bearer token.
    pub token_type: OAuthTokenType,

    /// The point in time at which the access token expires.
    pub expires_at: Option<Datetime>,
}

impl<S: BosStr> TokenSet<S> {
    /// Convert to an `Nsid` with a different backing type.
    pub fn convert<B: From<S> + BosStr>(self) -> TokenSet<B> {
        TokenSet {
            iss: self.iss.into(),
            sub: self.sub.convert(),
            aud: self.aud.into(),
            scope: self.scope.map(|s| s.into()),
            refresh_token: self.refresh_token.map(|t| t.into()),
            access_token: self.access_token.into(),
            token_type: self.token_type,
            expires_at: self.expires_at,
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for TokenSet<S>
where
    S::Output: BosStr,
{
    type Output = TokenSet<S::Output>;

    fn into_static(self) -> Self::Output {
        TokenSet {
            iss: self.iss.into_static(),
            sub: self.sub.into_static(),
            aud: self.aud.into_static(),
            scope: self.scope.into_static(),
            refresh_token: self.refresh_token.into_static(),
            access_token: self.access_token.into_static(),
            token_type: self.token_type,
            expires_at: self.expires_at.into_static(),
        }
    }
}
