use jacquard_common::{CowStr, IntoStatic, types::string::Language};
use serde::{Deserialize, Serialize};

/// Authorization server metadata, as returned from the
/// `.well-known/oauth-authorization-server` discovery document.
///
/// Defined by [RFC 8414](https://datatracker.ietf.org/doc/html/rfc8414#section-2)
/// with extensions from OpenID Connect Discovery, RFC 9126 (PAR), RFC 9207,
/// RFC 9449 (DPoP), and the ATProto client ID metadata document draft.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OAuthAuthorizationServerMetadata<'s> {
    /// The issuer identifier URL of the authorization server.
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc8414#section-2>
    #[serde(borrow)]
    pub issuer: CowStr<'s>,
    /// The URL of the authorization endpoint.
    pub authorization_endpoint: CowStr<'s>, // optional?
    /// The URL of the token endpoint.
    pub token_endpoint: CowStr<'s>, // optional?
    /// URL of the authorization server's JWK Set document.
    pub jwks_uri: Option<CowStr<'s>>,
    /// URL of the dynamic client registration endpoint, if supported.
    pub registration_endpoint: Option<CowStr<'s>>,
    /// List of OAuth 2.0 scope values the server supports.
    pub scopes_supported: Vec<CowStr<'s>>,
    /// List of OAuth 2.0 response type values the server supports.
    pub response_types_supported: Vec<CowStr<'s>>,
    /// List of OAuth 2.0 response mode values the server supports.
    pub response_modes_supported: Option<Vec<CowStr<'s>>>,
    /// List of OAuth 2.0 grant type values the server supports.
    pub grant_types_supported: Option<Vec<CowStr<'s>>>,
    /// List of client authentication methods supported at the token endpoint.
    pub token_endpoint_auth_methods_supported: Option<Vec<CowStr<'s>>>,
    /// List of JWS signing algorithms supported for token endpoint auth.
    pub token_endpoint_auth_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    /// URL of a page with human-readable information about the server.
    pub service_documentation: Option<CowStr<'s>>,
    /// BCP 47 language tags for UI locales the server supports.
    pub ui_locales_supported: Option<Vec<Language>>,
    /// URL of the authorization server's privacy policy.
    pub op_policy_uri: Option<CowStr<'s>>,
    /// URL of the authorization server's terms of service.
    pub op_tos_uri: Option<CowStr<'s>>,
    /// URL of the token revocation endpoint (RFC 7009).
    pub revocation_endpoint: Option<CowStr<'s>>,
    /// List of client authentication methods supported at the revocation endpoint.
    pub revocation_endpoint_auth_methods_supported: Option<Vec<CowStr<'s>>>,
    /// List of JWS signing algorithms supported for revocation endpoint auth.
    pub revocation_endpoint_auth_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    /// URL of the token introspection endpoint (RFC 7662).
    pub introspection_endpoint: Option<CowStr<'s>>,
    /// List of client authentication methods supported at the introspection endpoint.
    pub introspection_endpoint_auth_methods_supported: Option<Vec<CowStr<'s>>>,
    /// List of JWS signing algorithms supported for introspection endpoint auth.
    pub introspection_endpoint_auth_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    /// PKCE code challenge methods supported by the server.
    pub code_challenge_methods_supported: Option<Vec<CowStr<'s>>>,

    /// Subject identifier types supported (`public` or `pairwise`).
    ///
    /// <https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderMetadata>
    pub subject_types_supported: Option<Vec<CowStr<'s>>>,
    /// If `true`, clients must pre-register `request_uri` values.
    pub require_request_uri_registration: Option<bool>,

    /// URL of the Pushed Authorization Request (PAR) endpoint (RFC 9126).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc9126#section-5>
    pub pushed_authorization_request_endpoint: Option<CowStr<'s>>,
    /// If `true`, all authorization requests must use PAR.
    pub require_pushed_authorization_requests: Option<bool>,

    /// If `true`, the server includes `iss` in authorization responses to prevent mix-up attacks.
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc9207#section-3>
    pub authorization_response_iss_parameter_supported: Option<bool>,

    /// DPoP JWS signing algorithms supported by this server (RFC 9449).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc9449#section-5.1>
    pub dpop_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,

    /// If `true`, the server supports the ATProto client ID metadata document extension.
    ///
    /// <https://drafts.aaronpk.com/draft-parecki-oauth-client-id-metadata-document/draft-parecki-oauth-client-id-metadata-document.html#section-5>
    pub client_id_metadata_document_supported: Option<bool>,

    /// Protected resources associated with this authorization server.
    ///
    /// <https://datatracker.ietf.org/doc/html/draft-ietf-oauth-resource-metadata-08#name-authorization-server-metada>
    pub protected_resources: Option<Vec<CowStr<'s>>>,
}

/// Protected resource metadata, returned from `.well-known/oauth-protected-resource`.
///
/// Allows clients to discover which authorization servers protect a given resource
/// and what scopes and bearer methods are accepted. Defined by
/// [draft-ietf-oauth-resource-metadata](https://datatracker.ietf.org/doc/draft-ietf-oauth-resource-metadata/).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OAuthProtectedResourceMetadata<'s> {
    /// The URL of the protected resource itself.
    #[serde(borrow)]
    pub resource: CowStr<'s>,
    /// URLs of authorization servers that can issue tokens for this resource.
    pub authorization_servers: Option<Vec<CowStr<'s>>>,
    /// URL of the resource server's JWK Set document.
    pub jwks_uri: Option<CowStr<'s>>,
    /// List of OAuth 2.0 scope values the resource server supports.
    pub scopes_supported: Vec<CowStr<'s>>,
    /// Bearer token presentation methods supported (`header`, `body`, `query`).
    pub bearer_methods_supported: Option<Vec<CowStr<'s>>>,
    /// JWS signing algorithms supported for resource-bound tokens.
    pub resource_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    /// URL of a page with human-readable information about the resource.
    pub resource_documentation: Option<CowStr<'s>>,
    /// URL of the resource server's privacy policy.
    pub resource_policy_uri: Option<CowStr<'s>>,
    /// URL of the resource server's terms of service.
    pub resource_tos_uri: Option<CowStr<'s>>,
}

impl IntoStatic for OAuthProtectedResourceMetadata<'_> {
    type Output = OAuthProtectedResourceMetadata<'static>;
    fn into_static(self) -> Self::Output {
        OAuthProtectedResourceMetadata {
            resource: self.resource.into_static(),
            authorization_servers: self.authorization_servers.into_static(),
            jwks_uri: self.jwks_uri.map(|v| v.into_static()),
            scopes_supported: self.scopes_supported.into_static(),
            bearer_methods_supported: self.bearer_methods_supported.map(|v| v.into_static()),
            resource_signing_alg_values_supported: self
                .resource_signing_alg_values_supported
                .map(|v| v.into_static()),
            resource_documentation: self.resource_documentation.map(|v| v.into_static()),
            resource_policy_uri: self.resource_policy_uri.map(|v| v.into_static()),
            resource_tos_uri: self.resource_tos_uri.map(|v| v.into_static()),
        }
    }
}

impl IntoStatic for OAuthAuthorizationServerMetadata<'_> {
    type Output = OAuthAuthorizationServerMetadata<'static>;
    fn into_static(self) -> Self::Output {
        OAuthAuthorizationServerMetadata {
            issuer: self.issuer.into_static(),
            authorization_endpoint: self.authorization_endpoint.into_static(),
            token_endpoint: self.token_endpoint.into_static(),
            jwks_uri: self.jwks_uri.into_static(),
            registration_endpoint: self.registration_endpoint.into_static(),
            scopes_supported: self.scopes_supported.into_static(),
            response_types_supported: self.response_types_supported.into_static(),
            response_modes_supported: self.response_modes_supported.into_static(),
            grant_types_supported: self.grant_types_supported.into_static(),
            token_endpoint_auth_methods_supported: self
                .token_endpoint_auth_methods_supported
                .into_static(),
            token_endpoint_auth_signing_alg_values_supported: self
                .token_endpoint_auth_signing_alg_values_supported
                .into_static(),
            service_documentation: self.service_documentation.into_static(),
            ui_locales_supported: self.ui_locales_supported.into_static(),
            op_policy_uri: self.op_policy_uri.into_static(),
            op_tos_uri: self.op_tos_uri.into_static(),
            revocation_endpoint: self.revocation_endpoint.into_static(),
            revocation_endpoint_auth_methods_supported: self
                .revocation_endpoint_auth_methods_supported
                .into_static(),
            revocation_endpoint_auth_signing_alg_values_supported: self
                .revocation_endpoint_auth_signing_alg_values_supported
                .into_static(),
            introspection_endpoint: self.introspection_endpoint.into_static(),
            introspection_endpoint_auth_methods_supported: self
                .introspection_endpoint_auth_methods_supported
                .into_static(),
            introspection_endpoint_auth_signing_alg_values_supported: self
                .introspection_endpoint_auth_signing_alg_values_supported
                .into_static(),
            code_challenge_methods_supported: self.code_challenge_methods_supported.into_static(),
            subject_types_supported: self.subject_types_supported.into_static(),
            require_request_uri_registration: self.require_request_uri_registration.into_static(),
            pushed_authorization_request_endpoint: self
                .pushed_authorization_request_endpoint
                .into_static(),
            require_pushed_authorization_requests: self
                .require_pushed_authorization_requests
                .into_static(),
            authorization_response_iss_parameter_supported: self
                .authorization_response_iss_parameter_supported
                .into_static(),
            dpop_signing_alg_values_supported: self.dpop_signing_alg_values_supported.into_static(),
            client_id_metadata_document_supported: self
                .client_id_metadata_document_supported
                .into_static(),
            protected_resources: self.protected_resources.into_static(),
        }
    }
}
