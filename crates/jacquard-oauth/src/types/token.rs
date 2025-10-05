use super::response::OAuthTokenType;
use jacquard_common::types::string::{Datetime, Did};
use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TokenSet<'s> {
    #[serde(borrow)]
    pub iss: CowStr<'s>,
    pub sub: Did<'s>,
    pub aud: CowStr<'s>,
    pub scope: Option<CowStr<'s>>,

    pub refresh_token: Option<CowStr<'s>>,
    pub access_token: CowStr<'s>,
    pub token_type: OAuthTokenType,

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
