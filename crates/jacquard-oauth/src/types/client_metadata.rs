use jacquard_common::{CowStr, IntoStatic};
use jose_jwk::JwkSet;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthClientMetadata<'c> {
    pub client_id: Url,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<Url>,
    pub redirect_uris: Vec<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub scope: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types: Option<Vec<CowStr<'c>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<CowStr<'c>>,
    // https://datatracker.ietf.org/doc/html/rfc9449#section-5.2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpop_bound_access_tokens: Option<bool>,
    // https://datatracker.ietf.org/doc/html/rfc7591#section-2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks: Option<JwkSet>,
    // https://openid.net/specs/openid-connect-registration-1_0.html#ClientMetadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_signing_alg: Option<CowStr<'c>>,
}

impl OAuthClientMetadata<'_> {}

impl IntoStatic for OAuthClientMetadata<'_> {
    type Output = OAuthClientMetadata<'static>;

    fn into_static(self) -> Self::Output {
        OAuthClientMetadata {
            client_id: self.client_id,
            client_uri: self.client_uri,
            redirect_uris: self.redirect_uris,
            scope: self.scope.map(|scope| scope.into_static()),
            grant_types: self.grant_types.map(|types| types.into_static()),
            token_endpoint_auth_method: self
                .token_endpoint_auth_method
                .map(|method| method.into_static()),
            dpop_bound_access_tokens: self.dpop_bound_access_tokens,
            jwks_uri: self.jwks_uri,
            jwks: self.jwks,
            token_endpoint_auth_signing_alg: self
                .token_endpoint_auth_signing_alg
                .map(|alg| alg.into_static()),
        }
    }
}
