use jacquard_common::{CowStr, IntoStatic};
use jose_jwk::JwkSet;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// OAuth 2.1 client metadata, used in the ATProto client ID metadata document.
///
/// In ATProto's OAuth profile, clients are identified by a URL that serves this
/// metadata document. Fields follow RFC 7591 (Dynamic Client Registration),
/// RFC 9449 (DPoP), and OpenID Connect Registration.
///
/// <https://atproto.com/specs/oauth>
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OAuthClientMetadata<'c> {
    /// The client identifier, typically a URL pointing to this metadata document.
    pub client_id: CowStr<'c>,
    /// URL of the client's home page, used for display purposes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<CowStr<'c>>,
    /// List of redirect URIs the authorization server may send callbacks to.
    pub redirect_uris: Vec<CowStr<'c>>,
    /// Space-separated list of scopes the client is allowed to request.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub scope: Option<CowStr<'c>>,
    /// Application type (`web` or `native`), used to enforce redirect URI constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_type: Option<CowStr<'c>>,
    /// OAuth 2.0 grant types the client will use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types: Option<Vec<CowStr<'c>>>,
    /// Authentication method the client uses at the token endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<CowStr<'c>>,
    /// Response types the client will use in authorization requests.
    pub response_types: Vec<CowStr<'c>>,
    /// If `true`, the client requires DPoP-bound access tokens (RFC 9449 §5.2).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc9449#section-5.2>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpop_bound_access_tokens: Option<bool>,
    /// URL of the client's JWK Set document for verifying signed requests (RFC 7591 §2).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc7591#section-2>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<CowStr<'c>>,
    /// Inline JWK Set for verifying signed requests, alternative to `jwks_uri`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks: Option<JwkSet>,
    /// JWS algorithm the client uses to sign token endpoint authentication assertions.
    ///
    /// <https://openid.net/specs/openid-connect-registration-1_0.html#ClientMetadata>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_signing_alg: Option<CowStr<'c>>,
    /// Human-readable name of the client, shown to users during authorization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<SmolStr>,
    /// URL of the client's logo image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<CowStr<'c>>,
    /// URL of the client's terms of service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tos_uri: Option<CowStr<'c>>,
    /// URL of the client's privacy policy.
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
