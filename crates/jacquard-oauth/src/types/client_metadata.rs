use jacquard_common::IntoStatic;
use jacquard_common::bos::{BosStr, DefaultStr};
use jose_jwk::JwkSet;
use serde::{Deserialize, Serialize};

/// OAuth 2.1 client metadata, used in the ATProto client ID metadata document.
///
/// In ATProto's OAuth profile, clients are identified by a URL that serves this
/// metadata document. Fields follow RFC 7591 (Dynamic Client Registration),
/// RFC 9449 (DPoP), and OpenID Connect Registration.
///
/// <https://atproto.com/specs/oauth>
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub struct OAuthClientMetadata<S: BosStr = DefaultStr> {
    /// The client identifier, typically a URL pointing to this metadata document.
    pub client_id: S,
    /// URL of the client's home page, used for display purposes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<S>,
    /// List of redirect URIs the authorization server may send callbacks to.
    pub redirect_uris: Vec<S>,
    /// Space-separated list of scopes the client is allowed to request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<S>,
    /// Application type (`web` or `native`), used to enforce redirect URI constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_type: Option<S>,
    /// OAuth 2.0 grant types the client will use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types: Option<Vec<S>>,
    /// Authentication method the client uses at the token endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<S>,
    /// Response types the client will use in authorization requests.
    pub response_types: Vec<S>,
    /// If `true`, the client requires DPoP-bound access tokens (RFC 9449 §5.2).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc9449#section-5.2>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpop_bound_access_tokens: Option<bool>,
    /// URL of the client's JWK Set document for verifying signed requests (RFC 7591 §2).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc7591#section-2>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<S>,
    /// Inline JWK Set for verifying signed requests, alternative to `jwks_uri`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks: Option<JwkSet>,
    /// JWS algorithm the client uses to sign token endpoint authentication assertions.
    ///
    /// <https://openid.net/specs/openid-connect-registration-1_0.html#ClientMetadata>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_signing_alg: Option<S>,
    /// Human-readable name of the client, shown to users during authorization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<S>,
    /// URL of the client's logo image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<S>,
    /// URL of the client's terms of service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tos_uri: Option<S>,
    /// URL of the client's privacy policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_policy_uri: Option<S>,
}

impl<S: BosStr> OAuthClientMetadata<S> {}

impl<S: BosStr + IntoStatic> IntoStatic for OAuthClientMetadata<S>
where
    S::Output: BosStr,
{
    type Output = OAuthClientMetadata<S::Output>;

    fn into_static(self) -> Self::Output {
        OAuthClientMetadata {
            client_id: self.client_id.into_static(),
            client_uri: self.client_uri.into_static(),
            redirect_uris: self.redirect_uris.into_static(),
            scope: self.scope.into_static(),
            application_type: self.application_type.into_static(),
            grant_types: self.grant_types.into_static(),
            response_types: self.response_types.into_static(),
            token_endpoint_auth_method: self.token_endpoint_auth_method.into_static(),
            dpop_bound_access_tokens: self.dpop_bound_access_tokens,
            jwks_uri: self.jwks_uri.into_static(),
            jwks: self.jwks,
            token_endpoint_auth_signing_alg: self.token_endpoint_auth_signing_alg.into_static(),
            client_name: self.client_name.into_static(),
            logo_uri: self.logo_uri.into_static(),
            tos_uri: self.tos_uri.into_static(),
            privacy_policy_uri: self.privacy_policy_uri.into_static(),
        }
    }
}
