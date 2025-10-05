use jacquard_common::{CowStr, IntoStatic, types::string::Language};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OAuthAuthorizationServerMetadata<'s> {
    // https://datatracker.ietf.org/doc/html/rfc8414#section-2
    #[serde(borrow)]
    pub issuer: CowStr<'s>,
    pub authorization_endpoint: CowStr<'s>, // optional?
    pub token_endpoint: CowStr<'s>,         // optional?
    pub jwks_uri: Option<CowStr<'s>>,
    pub registration_endpoint: Option<CowStr<'s>>,
    pub scopes_supported: Vec<CowStr<'s>>,
    pub response_types_supported: Vec<CowStr<'s>>,
    pub response_modes_supported: Option<Vec<CowStr<'s>>>,
    pub grant_types_supported: Option<Vec<CowStr<'s>>>,
    pub token_endpoint_auth_methods_supported: Option<Vec<CowStr<'s>>>,
    pub token_endpoint_auth_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    pub service_documentation: Option<CowStr<'s>>,
    pub ui_locales_supported: Option<Vec<Language>>,
    pub op_policy_uri: Option<CowStr<'s>>,
    pub op_tos_uri: Option<CowStr<'s>>,
    pub revocation_endpoint: Option<CowStr<'s>>,
    pub revocation_endpoint_auth_methods_supported: Option<Vec<CowStr<'s>>>,
    pub revocation_endpoint_auth_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    pub introspection_endpoint: Option<CowStr<'s>>,
    pub introspection_endpoint_auth_methods_supported: Option<Vec<CowStr<'s>>>,
    pub introspection_endpoint_auth_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    pub code_challenge_methods_supported: Option<Vec<CowStr<'s>>>,

    // https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderMetadata
    pub subject_types_supported: Option<Vec<CowStr<'s>>>,
    pub require_request_uri_registration: Option<bool>,

    // https://datatracker.ietf.org/doc/html/rfc9126#section-5
    pub pushed_authorization_request_endpoint: Option<CowStr<'s>>,
    pub require_pushed_authorization_requests: Option<bool>,

    // https://datatracker.ietf.org/doc/html/rfc9207#section-3
    pub authorization_response_iss_parameter_supported: Option<bool>,

    // https://datatracker.ietf.org/doc/html/rfc9449#section-5.1
    pub dpop_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,

    // https://drafts.aaronpk.com/draft-parecki-oauth-client-id-metadata-document/draft-parecki-oauth-client-id-metadata-document.html#section-5
    pub client_id_metadata_document_supported: Option<bool>,

    // https://datatracker.ietf.org/doc/html/draft-ietf-oauth-resource-metadata-08#name-authorization-server-metada
    pub protected_resources: Option<Vec<CowStr<'s>>>,
}

// https://datatracker.ietf.org/doc/draft-ietf-oauth-resource-metadata/
// https://datatracker.ietf.org/doc/html/draft-ietf-oauth-resource-metadata-08#section-2
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OAuthProtectedResourceMetadata<'s> {
    #[serde(borrow)]
    pub resource: CowStr<'s>,
    pub authorization_servers: Option<Vec<Url>>,
    pub jwks_uri: Option<CowStr<'s>>,
    pub scopes_supported: Vec<CowStr<'s>>,
    pub bearer_methods_supported: Option<Vec<CowStr<'s>>>,
    pub resource_signing_alg_values_supported: Option<Vec<CowStr<'s>>>,
    pub resource_documentation: Option<CowStr<'s>>,
    pub resource_policy_uri: Option<CowStr<'s>>,
    pub resource_tos_uri: Option<CowStr<'s>>,
}

impl IntoStatic for OAuthProtectedResourceMetadata<'_> {
    type Output = OAuthProtectedResourceMetadata<'static>;
    fn into_static(self) -> Self::Output {
        OAuthProtectedResourceMetadata {
            resource: self.resource.into_static(),
            authorization_servers: self.authorization_servers,
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
