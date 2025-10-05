use crate::types::TokenSet;

use jacquard_common::IntoStatic;
use jose_jwk::Key;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OauthSession<'s> {
    pub dpop_key: Key,
    #[serde(borrow)]
    pub token_set: TokenSet<'s>,
}

impl IntoStatic for OauthSession<'_> {
    type Output = OauthSession<'static>;

    fn into_static(self) -> Self::Output {
        OauthSession {
            dpop_key: self.dpop_key,
            token_set: self.token_set.into_static(),
        }
    }
}
