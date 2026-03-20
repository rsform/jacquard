use crate::types::OAuthClientMetadata;
use crate::{keyset::Keyset, scopes::Scope};
use jacquard_common::cowstr::ToCowStr;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};
use thiserror::Error;

/// Errors that can occur when building AT Protocol OAuth client metadata.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The `client_id` is not a valid URL.
    #[error("`client_id` must be a valid URL")]
    InvalidClientId,
    /// The `grant_types` list does not include `authorization_code`, which is required by atproto.
    #[error("`grant_types` must include `authorization_code`")]
    InvalidGrantTypes,
    /// The `scope` list does not include `atproto`, which is required for all atproto clients.
    #[error("`scope` must not include `atproto`")]
    InvalidScope,
    /// No redirect URIs were provided; at least one is required.
    #[error("`redirect_uris` must not be empty")]
    EmptyRedirectUris,
    /// The `private_key_jwt` auth method was requested but no JWK keys were provided.
    #[error("`private_key_jwt` auth method requires `jwks` keys")]
    EmptyJwks,
    /// Signing algorithm mismatch: `private_key_jwt` requires `token_endpoint_auth_signing_alg`,
    /// and non-`private_key_jwt` methods must not provide it.
    #[error(
        "`private_key_jwt` auth method requires `token_endpoint_auth_signing_alg`, otherwise must not be provided"
    )]
    AuthSigningAlg,
    /// HTML form serialization of the loopback `client_id` query string failed.
    #[error(transparent)]
    SerdeHtmlForm(#[from] serde_html_form::ser::Error),
    /// A localhost-specific validation error occurred.
    #[error(transparent)]
    LocalhostClient(#[from] LocalhostClientError),
}

/// Errors specific to validating a loopback (localhost) OAuth client's redirect URIs.
///
/// The AT Protocol spec has specific requirements for loopback clients: redirect URIs must
/// use the `http` scheme and must point to actual loopback addresses (not the hostname `localhost`).
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LocalhostClientError {
    /// The redirect URI could not be parsed.
    #[error("invalid redirect_uri: {0}")]
    Invalid(#[from] jacquard_common::deps::fluent_uri::ParseError),
    /// Loopback redirect URIs must use `http:`, not `https:` or any other scheme.
    #[error("loopback client_id must use `http:` redirect_uri")]
    NotHttpScheme,
    /// The hostname `localhost` is not allowed; use a numeric loopback address instead.
    #[error("loopback client_id must not use `localhost` as redirect_uri hostname")]
    Localhost,
    /// The redirect URI host is not a loopback address (127.x.x.x or ::1).
    #[error("loopback client_id must not use loopback addresses as redirect_uri")]
    NotLoopbackHost,
}

/// Convenience result type for AT Protocol client metadata operations.
pub type Result<T> = core::result::Result<T, Error>;

/// The token endpoint authentication method for an OAuth client.
///
/// AT Protocol clients either authenticate with no client secret (public/loopback clients)
/// or with a private key JWT signed by a key from the client's JWK set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// No client authentication; used for public and loopback clients.
    None,
    /// Authenticate using a JWT signed with a private key from the client's JWK set.
    /// <https://openid.net/specs/openid-connect-core-1_0.html#ClientAuthentication>
    PrivateKeyJwt,
}

impl From<AuthMethod> for CowStr<'static> {
    fn from(value: AuthMethod) -> Self {
        match value {
            AuthMethod::None => CowStr::new_static("none"),
            AuthMethod::PrivateKeyJwt => CowStr::new_static("private_key_jwt"),
        }
    }
}

/// OAuth 2.0 grant types supported by AT Protocol clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    /// Standard authorization code grant, required by atproto.
    AuthorizationCode,
    /// Refresh token grant, used to obtain new access tokens without re-authorization.
    RefreshToken,
}

impl From<GrantType> for CowStr<'static> {
    fn from(value: GrantType) -> Self {
        match value {
            GrantType::AuthorizationCode => CowStr::new_static("authorization_code"),
            GrantType::RefreshToken => CowStr::new_static("refresh_token"),
        }
    }
}

/// AT Protocol-specific OAuth client metadata, used to describe a client before converting to
/// the generic [`OAuthClientMetadata`] format for server registration.
///
/// This type provides a validated, atproto-aware view of client registration data, with
/// typed fields for URIs and scopes rather than raw strings. Use [`atproto_client_metadata`]
/// to convert this into the wire format expected by OAuth servers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtprotoClientMetadata<'m> {
    /// The unique identifier for this client, typically the URL of its metadata document.
    pub client_id: Uri<String>,
    /// The URI of the client's homepage or information page.
    pub client_uri: Option<Uri<String>>,
    /// The list of allowed redirect URIs for the authorization code flow.
    pub redirect_uris: Vec<Uri<String>>,
    /// The grant types this client will use.
    pub grant_types: Vec<GrantType>,
    /// The OAuth scopes this client requests; must include `atproto`.
    #[serde(borrow)]
    pub scopes: Vec<Scope<'m>>,
    /// URI pointing to the client's JWK Set; mutually exclusive with inline `jwks`.
    pub jwks_uri: Option<Uri<String>>,
    /// Human-readable display name for the client.
    pub client_name: Option<SmolStr>,
    /// URI of the client's logo image.
    pub logo_uri: Option<Uri<String>>,
    /// URI of the client's terms of service document.
    pub tos_uri: Option<Uri<String>>,
    /// URI of the client's privacy policy document.
    pub privacy_policy_uri: Option<Uri<String>>,
}

impl<'m> IntoStatic for AtprotoClientMetadata<'m> {
    type Output = AtprotoClientMetadata<'static>;
    fn into_static(self) -> AtprotoClientMetadata<'static> {
        AtprotoClientMetadata {
            client_id: self.client_id,
            client_uri: self.client_uri,
            redirect_uris: self.redirect_uris,
            grant_types: self.grant_types,
            scopes: self.scopes.into_static(),
            jwks_uri: self.jwks_uri,
            client_name: self.client_name,
            logo_uri: self.logo_uri,
            tos_uri: self.tos_uri,
            privacy_policy_uri: None,
        }
    }
}

impl<'m> AtprotoClientMetadata<'m> {
    /// Attach optional production branding fields to the metadata.
    ///
    /// Chainable builder method for setting display name, logo, and policy URLs after
    /// constructing the base metadata.
    pub fn with_prod_info(
        mut self,
        client_name: &str,
        logo_uri: Option<Uri<String>>,
        tos_uri: Option<Uri<String>>,
        privacy_policy_uri: Option<Uri<String>>,
    ) -> Self {
        self.client_name = Some(client_name.to_smolstr());
        self.logo_uri = logo_uri;
        self.tos_uri = tos_uri;
        self.privacy_policy_uri = privacy_policy_uri;
        self
    }

    /// Create a default loopback client metadata with the `atproto` and `transition:generic` scopes.
    ///
    /// This is a convenience constructor for local development and CLI tools. The resulting
    /// metadata uses `http://localhost` as the `client_id` with both IPv4 and IPv6 loopback
    /// redirect URIs.
    pub fn default_localhost() -> Self {
        Self::new_localhost(
            None,
            Some(Scope::parse_multiple("atproto transition:generic").unwrap()),
        )
    }

    /// Create loopback client metadata with optional custom redirect URIs and scopes.
    ///
    /// Encodes non-default redirect URIs and scopes into the `client_id` query string as
    /// required by the AT Protocol loopback client specification. When `redirect_uris` or
    /// `scopes` are `None`, sensible defaults (IPv4 + IPv6 loopback addresses, `atproto` scope)
    /// are used.
    pub fn new_localhost(
        redirect_uris: Option<Vec<Uri<String>>>,
        scopes: Option<Vec<Scope<'static>>>,
    ) -> AtprotoClientMetadata<'static> {
        // determine client_id
        #[derive(serde::Serialize)]
        struct Parameters<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            redirect_uri: Option<Vec<CowStr<'a>>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            scope: Option<CowStr<'a>>,
        }
        let redir_str = redirect_uris.as_ref().map(|uris| {
            uris.iter()
                .map(|u| u.as_str().trim_end_matches("/").to_cowstr().into_static())
                .collect()
        });
        let query = serde_html_form::to_string(Parameters {
            redirect_uri: redir_str,
            scope: scopes
                .as_ref()
                .map(|s| Scope::serialize_multiple(s.as_slice())),
        })
        .ok();
        let mut client_id = String::from("http://localhost/");
        if let Some(query) = query
            && !query.is_empty()
        {
            client_id.push_str(&format!("?{query}"));
        }
        AtprotoClientMetadata {
            client_id: Uri::parse(client_id).unwrap(),
            client_uri: None,
            redirect_uris: redirect_uris.unwrap_or(vec![
                Uri::parse("http://127.0.0.1".to_string()).unwrap(),
                Uri::parse("http://[::1]".to_string()).unwrap(),
            ]),
            grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
            scopes: scopes.unwrap_or(vec![Scope::Atproto]),
            jwks_uri: None,
            client_name: None,
            logo_uri: None,
            tos_uri: None,
            privacy_policy_uri: None,
        }
    }
}

/// Convert [`AtprotoClientMetadata`] into the [`OAuthClientMetadata`] wire format.
///
/// Validates all atproto-specific constraints (required scopes, grant types, redirect URIs),
/// selects the appropriate `token_endpoint_auth_method` based on whether a keyset is provided,
/// and serializes scopes and grant types into their string representations. Returns an error
/// if any required field is missing or invalid.
pub fn atproto_client_metadata<'m>(
    metadata: AtprotoClientMetadata<'m>,
    keyset: &Option<Keyset>,
) -> Result<OAuthClientMetadata<'static>> {
    let is_loopback = metadata.client_id.scheme().as_str() == "http"
        && metadata.client_id.authority().map(|a| a.host()) == Some("localhost");
    let application_type = if is_loopback {
        Some(CowStr::new_static("native"))
    } else {
        Some(CowStr::new_static("web"))
    };
    if metadata.redirect_uris.is_empty() {
        return Err(Error::EmptyRedirectUris);
    }
    if !metadata.grant_types.contains(&GrantType::AuthorizationCode) {
        return Err(Error::InvalidGrantTypes);
    }
    if !metadata.scopes.contains(&Scope::Atproto) {
        return Err(Error::InvalidScope);
    }
    let (auth_method, jwks_uri, jwks) = if let Some(keyset) = keyset {
        let jwks = if metadata.jwks_uri.is_none() {
            Some(keyset.public_jwks())
        } else {
            None
        };
        (AuthMethod::PrivateKeyJwt, metadata.jwks_uri, jwks)
    } else {
        (AuthMethod::None, None, None)
    };
    let client_id = metadata
        .client_id
        .as_str()
        .trim_end_matches("/")
        .to_string();
    let client_uri = metadata
        .client_uri
        .as_ref()
        .map(|u| u.as_str().trim_end_matches("/").to_string().into());
    let redirect_uris = metadata
        .redirect_uris
        .iter()
        .map(|u| u.as_str().trim_end_matches("/").to_string().into())
        .collect();
    let jwks_uri = jwks_uri.map(|u| u.as_str().trim_end_matches("/").to_string().into());
    Ok(OAuthClientMetadata {
        client_id: client_id.into(),
        client_uri,
        redirect_uris,
        application_type,
        token_endpoint_auth_method: Some(auth_method.into()),
        grant_types: Some(metadata.grant_types.into_iter().map(|v| v.into()).collect()),
        response_types: vec!["code".to_cowstr()],
        scope: Some(Scope::serialize_multiple(metadata.scopes.as_slice())),
        dpop_bound_access_tokens: Some(true),
        jwks_uri,
        jwks,
        token_endpoint_auth_signing_alg: if keyset.is_some() {
            Some(CowStr::new_static("ES256"))
        } else {
            None
        },
        client_name: metadata.client_name,
        logo_uri: metadata
            .logo_uri
            .as_ref()
            .map(|u| u.as_str().to_string().into()),
        tos_uri: metadata
            .tos_uri
            .as_ref()
            .map(|u| u.as_str().to_string().into()),
        privacy_policy_uri: metadata
            .privacy_policy_uri
            .as_ref()
            .map(|u| u.as_str().to_string().into()),
    })
}

#[cfg(test)]
mod tests {
    use crate::scopes::TransitionScope;

    use super::*;
    use elliptic_curve::SecretKey;
    use jose_jwk::{Jwk, Key, Parameters};
    use p256::pkcs8::DecodePrivateKey;

    const PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgED1AAgC7Fc9kPh5T
4i4Tn+z+tc47W1zYgzXtyjJtD92hRANCAAT80DqC+Z/JpTO7/pkPBmWqIV1IGh1P
gbGGr0pN+oSing7cZ0169JaRHTNh+0LNQXrFobInX6cj95FzEdRyT4T3
-----END PRIVATE KEY-----"#;

    #[test]
    fn test_localhost_client_metadata_default() {
        assert_eq!(
            atproto_client_metadata(AtprotoClientMetadata::new_localhost(None, None), &None)
                .unwrap(),
            OAuthClientMetadata {
                client_id: CowStr::new_static("http://localhost"),
                client_uri: None,
                redirect_uris: vec![
                    CowStr::new_static("http://127.0.0.1"),
                    CowStr::new_static("http://[::1]"),
                ],
                application_type: Some(CowStr::new_static("native")),
                scope: Some(CowStr::new_static("atproto")),
                grant_types: Some(vec![
                    "authorization_code".to_cowstr(),
                    "refresh_token".to_cowstr()
                ]),
                response_types: vec!["code".to_cowstr()],
                token_endpoint_auth_method: Some(AuthMethod::None.into()),
                dpop_bound_access_tokens: Some(true),
                jwks_uri: None,
                jwks: None,
                token_endpoint_auth_signing_alg: None,
                tos_uri: None,
                privacy_policy_uri: None,
                client_name: None,
                logo_uri: None,
            }
        );
    }

    #[test]
    fn test_localhost_client_metadata_custom() {
        assert_eq!(
            atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![
                        Uri::parse("http://127.0.0.1/callback".to_string()).unwrap(),
                        Uri::parse("http://[::1]/callback".to_string()).unwrap(),
                    ]),
                    Some(vec![
                        Scope::Atproto,
                        Scope::Transition(TransitionScope::Generic),
                        Scope::parse("account:email").unwrap()
                    ])
                ),
                &None
            )
            .expect("failed to convert metadata"),
            OAuthClientMetadata {
                client_id: CowStr::new_static(
                    "http://localhost/?redirect_uri=http%3A%2F%2F127.0.0.1%2Fcallback&redirect_uri=http%3A%2F%2F%5B%3A%3A1%5D%2Fcallback&scope=account%3Aemail+atproto+transition%3Ageneric"
                ),
                client_uri: None,
                redirect_uris: vec![
                    CowStr::new_static("http://127.0.0.1/callback"),
                    CowStr::new_static("http://[::1]/callback"),
                ],
                scope: Some(CowStr::new_static(
                    "account:email atproto transition:generic"
                )),
                application_type: Some(CowStr::new_static("native")),
                grant_types: Some(vec![
                    "authorization_code".to_cowstr(),
                    "refresh_token".to_cowstr()
                ]),
                response_types: vec!["code".to_cowstr()],
                token_endpoint_auth_method: Some(AuthMethod::None.into()),
                dpop_bound_access_tokens: Some(true),
                jwks_uri: None,
                jwks: None,
                token_endpoint_auth_signing_alg: None,
                tos_uri: None,
                privacy_policy_uri: None,
                client_name: None,
                logo_uri: None,
            }
        );
    }

    #[test]
    fn test_localhost_client_metadata_invalid() {
        // Invalid inputs are coerced to http://localhost rather than failing
        {
            let out = atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![Uri::parse("https://127.0.0.1".to_string()).unwrap()]),
                    None,
                ),
                &None,
            )
            .expect("should coerce to 127.0.0.1");
            assert_eq!(
                out,
                OAuthClientMetadata {
                    client_id: CowStr::new_static(
                        "http://localhost/?redirect_uri=https%3A%2F%2F127.0.0.1"
                    ),
                    application_type: Some(CowStr::new_static("native")),
                    client_uri: None,
                    redirect_uris: vec![CowStr::new_static("https://127.0.0.1")],
                    scope: Some(CowStr::new_static("atproto")),
                    grant_types: Some(vec![
                        "authorization_code".to_cowstr(),
                        "refresh_token".to_cowstr()
                    ]),
                    response_types: vec!["code".to_cowstr()],
                    token_endpoint_auth_method: Some(AuthMethod::None.into()),
                    dpop_bound_access_tokens: Some(true),
                    jwks_uri: None,
                    jwks: None,
                    token_endpoint_auth_signing_alg: None,
                    tos_uri: None,
                    privacy_policy_uri: None,
                    client_name: None,
                    logo_uri: None,
                }
            );
        }
        {
            let out = atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![
                        Uri::parse("http://localhost:8000".to_string()).unwrap(),
                    ]),
                    None,
                ),
                &None,
            )
            .expect("should coerce to 127.0.0.1");
            assert_eq!(
                out,
                OAuthClientMetadata {
                    client_id: CowStr::new_static(
                        "http://localhost/?redirect_uri=http%3A%2F%2Flocalhost%3A8000"
                    ),
                    client_uri: None,
                    redirect_uris: vec![CowStr::new_static("http://localhost:8000")],
                    scope: Some(CowStr::new_static("atproto")),
                    grant_types: Some(vec![
                        "authorization_code".to_cowstr(),
                        "refresh_token".to_cowstr()
                    ]),
                    application_type: Some(CowStr::new_static("native")),
                    response_types: vec!["code".to_cowstr()],
                    token_endpoint_auth_method: Some(AuthMethod::None.into()),
                    dpop_bound_access_tokens: Some(true),
                    jwks_uri: None,
                    jwks: None,
                    token_endpoint_auth_signing_alg: None,
                    tos_uri: None,
                    privacy_policy_uri: None,
                    client_name: None,
                    logo_uri: None,
                }
            );
        }
        {
            let out = atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![Uri::parse("http://192.168.0.0/".to_string()).unwrap()]),
                    None,
                ),
                &None,
            )
            .expect("should coerce to 127.0.0.1");
            assert_eq!(
                out,
                OAuthClientMetadata {
                    client_id: CowStr::new_static(
                        "http://localhost/?redirect_uri=http%3A%2F%2F192.168.0.0"
                    ),
                    client_uri: None,
                    redirect_uris: vec![CowStr::new_static("http://192.168.0.0")],
                    scope: Some(CowStr::new_static("atproto")),
                    grant_types: Some(vec![
                        "authorization_code".to_cowstr(),
                        "refresh_token".to_cowstr()
                    ]),
                    application_type: Some(CowStr::new_static("native")),
                    response_types: vec!["code".to_cowstr()],
                    token_endpoint_auth_method: Some(AuthMethod::None.into()),
                    dpop_bound_access_tokens: Some(true),
                    jwks_uri: None,
                    jwks: None,
                    token_endpoint_auth_signing_alg: None,
                    tos_uri: None,
                    privacy_policy_uri: None,
                    client_name: None,
                    logo_uri: None,
                }
            );
        }
    }

    #[test]
    fn test_client_metadata() {
        let metadata = AtprotoClientMetadata {
            client_id: Uri::parse("https://example.com/client_metadata.json".to_string()).unwrap(),
            client_uri: Some(Uri::parse("https://example.com".to_string()).unwrap()),
            redirect_uris: vec![Uri::parse("https://example.com/callback".to_string()).unwrap()],
            grant_types: vec![GrantType::AuthorizationCode],
            scopes: vec![Scope::Atproto],
            jwks_uri: None,
            client_name: None,
            logo_uri: None,
            tos_uri: None,
            privacy_policy_uri: None,
        };
        {
            // Non-loopback clients without a keyset should fail (must provide JWKS)
            let metadata = metadata.clone();
            let err = atproto_client_metadata(metadata, &None);
            assert!(err.is_ok());
        }
        {
            let metadata = metadata.clone();
            let secret_key = SecretKey::<p256::NistP256>::from_pkcs8_pem(PRIVATE_KEY)
                .expect("failed to parse private key");
            let keys = vec![Jwk {
                key: Key::from(&secret_key.into()),
                prm: Parameters {
                    kid: Some(String::from("kid00")),
                    ..Default::default()
                },
            }];
            let keyset = Keyset::try_from(keys.clone()).expect("failed to create keyset");
            assert_eq!(
                atproto_client_metadata(metadata, &Some(keyset.clone()))
                    .expect("failed to convert metadata"),
                OAuthClientMetadata {
                    client_id: CowStr::new_static("https://example.com/client_metadata.json"),
                    client_uri: Some(CowStr::new_static("https://example.com")),
                    redirect_uris: vec![CowStr::new_static("https://example.com/callback")],
                    application_type: Some(CowStr::new_static("web")),
                    scope: Some(CowStr::new_static("atproto")),
                    grant_types: Some(vec![CowStr::new_static("authorization_code")]),
                    token_endpoint_auth_method: Some(AuthMethod::PrivateKeyJwt.into()),
                    dpop_bound_access_tokens: Some(true),
                    response_types: vec!["code".to_cowstr()],
                    jwks_uri: None,
                    jwks: Some(keyset.public_jwks()),
                    token_endpoint_auth_signing_alg: Some(CowStr::new_static("ES256")),
                    client_name: None,
                    logo_uri: None,
                    tos_uri: None,
                    privacy_policy_uri: None,
                }
            );
        }
    }
}
