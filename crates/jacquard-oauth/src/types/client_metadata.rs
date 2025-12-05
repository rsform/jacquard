use jacquard_common::{CowStr, IntoStatic};
use jose_jwk::JwkSet;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthClientMetadata<'c> {
    pub client_id: CowStr<'c>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<CowStr<'c>>,
    pub redirect_uris: Vec<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub scope: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_type: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types: Option<Vec<CowStr<'c>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<CowStr<'c>>,
    pub response_types: Vec<CowStr<'c>>,
    // https://datatracker.ietf.org/doc/html/rfc9449#section-5.2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpop_bound_access_tokens: Option<bool>,
    // https://datatracker.ietf.org/doc/html/rfc7591#section-2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks: Option<JwkSet>,
    // https://openid.net/specs/openid-connect-registration-1_0.html#ClientMetadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_signing_alg: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tos_uri: Option<CowStr<'c>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_policy_uri: Option<CowStr<'c>>,
}

impl OAuthClientMetadata<'_> {}

impl IntoStatic for OAuthClientMetadata<'_> {
    type Output = OAuthClientMetadata<'static>;

    fn into_static(self) -> Self::Output {
        OAuthClientMetadata {
            client_id: self.client_id.into_static(),
            client_uri: self.client_uri.into_static(),
            redirect_uris: self.redirect_uris.into_static(),
            scope: self.scope.map(|scope| scope.into_static()),
            application_type: self.application_type.map(|app_type| app_type.into_static()),
            grant_types: self.grant_types.map(|types| types.into_static()),
            response_types: self.response_types.into_static(),
            token_endpoint_auth_method: self
                .token_endpoint_auth_method
                .map(|method| method.into_static()),
            dpop_bound_access_tokens: self.dpop_bound_access_tokens,
            jwks_uri: self.jwks_uri.into_static(),
            jwks: self.jwks,
            token_endpoint_auth_signing_alg: self
                .token_endpoint_auth_signing_alg
                .map(|alg| alg.into_static()),
            client_name: self.client_name,
            logo_uri: self.logo_uri.into_static(),
            tos_uri: self.tos_uri.into_static(),
            privacy_policy_uri: self.privacy_policy_uri.into_static(),
        }
    }
}
