use std::str::FromStr;

use crate::types::OAuthClientMetadata;
use crate::{keyset::Keyset, scopes::Scope};
use jacquard_common::CowStr;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{Host, Url};

#[derive(Error, Debug)]
pub enum Error {
    #[error("`client_id` must be a valid URL")]
    InvalidClientId,
    #[error("`grant_types` must include `authorization_code`")]
    InvalidGrantTypes,
    #[error("`scope` must not include `atproto`")]
    InvalidScope,
    #[error("`redirect_uris` must not be empty")]
    EmptyRedirectUris,
    #[error("`private_key_jwt` auth method requires `jwks` keys")]
    EmptyJwks,
    #[error(
        "`private_key_jwt` auth method requires `token_endpoint_auth_signing_alg`, otherwise must not be provided"
    )]
    AuthSigningAlg,
    #[error(transparent)]
    SerdeHtmlForm(#[from] serde_html_form::ser::Error),
    #[error(transparent)]
    LocalhostClient(#[from] LocalhostClientError),
}

#[derive(Error, Debug)]
pub enum LocalhostClientError {
    #[error("invalid redirect_uri: {0}")]
    Invalid(#[from] url::ParseError),
    #[error("loopback client_id must use `http:` redirect_uri")]
    NotHttpScheme,
    #[error("loopback client_id must not use `localhost` as redirect_uri hostname")]
    Localhost,
    #[error("loopback client_id must not use loopback addresses as redirect_uri")]
    NotLoopbackHost,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    None,
    // https://openid.net/specs/openid-connect-core-1_0.html#ClientAuthentication
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    AuthorizationCode,
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

pub fn localhost_client_metadata<'s>(
    redirect_uris: Option<Vec<Url>>,
    scopes: Option<&'s [Scope<'s>]>,
) -> Result<OAuthClientMetadata<'s>> {
    // validate redirect_uris
    if let Some(redirect_uris) = &redirect_uris {
        for redirect_uri in redirect_uris {
            if redirect_uri.scheme() != "http" {
                return Err(Error::LocalhostClient(LocalhostClientError::NotHttpScheme));
            }
            if redirect_uri.host().map(|h| h.to_owned()) == Some(Host::parse("localhost").unwrap())
            {
                return Err(Error::LocalhostClient(LocalhostClientError::Localhost));
            }
            if redirect_uri
                .host()
                .map(|h| h.to_owned())
                .map_or(true, |host| {
                    host != Host::parse("127.0.0.1").unwrap()
                        && host != Host::parse("[::1]").unwrap()
                })
            {
                return Err(Error::LocalhostClient(
                    LocalhostClientError::NotLoopbackHost,
                ));
            }
        }
    }
    // determine client_id
    #[derive(serde::Serialize)]
    struct Parameters<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        redirect_uri: Option<Vec<Url>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<CowStr<'a>>,
    }
    let query = serde_html_form::to_string(Parameters {
        redirect_uri: redirect_uris.clone(),
        scope: scopes.map(|s| Scope::serialize_multiple(s)),
    })?;
    let mut client_id = String::from("http://localhost");
    if !query.is_empty() {
        client_id.push_str(&format!("?{query}"));
    }
    Ok(OAuthClientMetadata {
        client_id: Url::parse(&client_id).unwrap(),
        client_uri: None,
        redirect_uris: redirect_uris.unwrap_or(vec![
            Url::from_str("http://127.0.0.1/").unwrap(),
            Url::from_str("http://[::1]/").unwrap(),
        ]),
        scope: None,
        grant_types: None, // will be set to `authorization_code` and `refresh_token`
        token_endpoint_auth_method: Some(CowStr::new_static("none")),
        dpop_bound_access_tokens: None, // will be set to `true`
        jwks_uri: None,
        jwks: None,
        token_endpoint_auth_signing_alg: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtprotoClientMetadata<'m> {
    pub client_id: Url,
    pub client_uri: Option<Url>,
    pub redirect_uris: Vec<Url>,
    pub token_endpoint_auth_method: AuthMethod,
    pub grant_types: Vec<GrantType>,
    pub scopes: Vec<Scope<'m>>,
    pub jwks_uri: Option<Url>,
    pub token_endpoint_auth_signing_alg: Option<CowStr<'m>>,
}

pub fn atproto_client_metadata<'m>(
    metadata: AtprotoClientMetadata<'m>,
    keyset: &Option<Keyset>,
) -> Result<OAuthClientMetadata<'m>> {
    if metadata.redirect_uris.is_empty() {
        return Err(Error::EmptyRedirectUris);
    }
    if !metadata.grant_types.contains(&GrantType::AuthorizationCode) {
        return Err(Error::InvalidGrantTypes);
    }
    if !metadata.scopes.contains(&Scope::Atproto) {
        return Err(Error::InvalidScope);
    }
    let (jwks_uri, mut jwks) = (metadata.jwks_uri, None);
    match metadata.token_endpoint_auth_method {
        AuthMethod::None => {
            if metadata.token_endpoint_auth_signing_alg.is_some() {
                return Err(Error::AuthSigningAlg);
            }
        }
        AuthMethod::PrivateKeyJwt => {
            if let Some(keyset) = keyset {
                if metadata.token_endpoint_auth_signing_alg.is_none() {
                    return Err(Error::AuthSigningAlg);
                }
                if jwks_uri.is_none() {
                    jwks = Some(keyset.public_jwks());
                }
            } else {
                return Err(Error::EmptyJwks);
            }
        }
    }
    Ok(OAuthClientMetadata {
        client_id: metadata.client_id,
        client_uri: metadata.client_uri,
        redirect_uris: metadata.redirect_uris,
        token_endpoint_auth_method: Some(metadata.token_endpoint_auth_method.into()),
        grant_types: Some(metadata.grant_types.into_iter().map(|v| v.into()).collect()),
        scope: Some(Scope::serialize_multiple(metadata.scopes.as_slice())),
        dpop_bound_access_tokens: Some(true),
        jwks_uri,
        jwks,
        token_endpoint_auth_signing_alg: metadata.token_endpoint_auth_signing_alg,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

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
            localhost_client_metadata(None, None).expect("failed to convert metadata"),
            OAuthClientMetadata {
                client_id: Url::from_str("http://localhost").unwrap(),
                client_uri: None,
                redirect_uris: vec![
                    Url::from_str("http://127.0.0.1/").unwrap(),
                    Url::from_str("http://[::1]/").unwrap(),
                ],
                scope: None,
                grant_types: None,
                token_endpoint_auth_method: Some(AuthMethod::None.into()),
                dpop_bound_access_tokens: None,
                jwks_uri: None,
                jwks: None,
                token_endpoint_auth_signing_alg: None,
            }
        );
    }

    #[test]
    fn test_localhost_client_metadata_custom() {
        assert_eq!(
            localhost_client_metadata(
                Some(vec![
                     Url::from_str("http://127.0.0.1/callback").unwrap(),
                     Url::from_str("http://[::1]/callback").unwrap(),
                ]),
                Some(
                    vec![
                        Scope::Atproto,
                        Scope::Transition(TransitionScope::Generic),
                        Scope::parse("account:email").unwrap()
                    ]
                    .as_slice()
                )
            )
            .expect("failed to convert metadata"),
            OAuthClientMetadata {
                client_id: Url::from_str(
                    "http://localhost?redirect_uri=http%3A%2F%2F127.0.0.1%2Fcallback&redirect_uri=http%3A%2F%2F%5B%3A%3A1%5D%2Fcallback&scope=account%3Aemail+atproto+transition%3Ageneric"
                ).unwrap(),
                client_uri: None,
                redirect_uris: vec![
                    Url::from_str("http://127.0.0.1/callback").unwrap(),
                    Url::from_str("http://[::1]/callback").unwrap(),
                ],
                scope: None,
                grant_types: None,
                token_endpoint_auth_method: Some(AuthMethod::None.into()),
                dpop_bound_access_tokens: None,
                jwks_uri: None,
                jwks: None,
                token_endpoint_auth_signing_alg: None,
            }
        );
    }

    #[test]
    fn test_localhost_client_metadata_invalid() {
        {
            let err = localhost_client_metadata(
                Some(vec![Url::from_str("https://127.0.0.1/").unwrap()]),
                None,
            )
            .expect_err("expected to fail");
            assert!(matches!(
                err,
                Error::LocalhostClient(LocalhostClientError::NotHttpScheme)
            ));
        }
        {
            let err = localhost_client_metadata(
                Some(vec![Url::from_str("http://localhost:8000/").unwrap()]),
                None,
            )
            .expect_err("expected to fail");
            assert!(matches!(
                err,
                Error::LocalhostClient(LocalhostClientError::Localhost)
            ));
        }
        {
            let err = localhost_client_metadata(
                Some(vec![Url::from_str("http://192.168.0.0/").unwrap()]),
                None,
            )
            .expect_err("expected to fail");
            assert!(matches!(
                err,
                Error::LocalhostClient(LocalhostClientError::NotLoopbackHost)
            ));
        }
    }

    #[test]
    fn test_client_metadata() {
        let metadata = AtprotoClientMetadata {
            client_id: Url::from_str("https://example.com/client_metadata.json").unwrap(),
            client_uri: Some(Url::from_str("https://example.com").unwrap()),
            redirect_uris: vec![Url::from_str("https://example.com/callback").unwrap()],
            token_endpoint_auth_method: AuthMethod::PrivateKeyJwt,
            grant_types: vec![GrantType::AuthorizationCode],
            scopes: vec![Scope::Atproto],
            jwks_uri: None,
            token_endpoint_auth_signing_alg: Some(CowStr::new_static("ES256")),
        };
        {
            let metadata = metadata.clone();
            let err = atproto_client_metadata(metadata, &None).expect_err("expected to fail");
            assert!(matches!(err, Error::EmptyJwks));
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
                    client_id: Url::from_str("https://example.com/client_metadata.json").unwrap(),
                    client_uri: Some(Url::from_str("https://example.com").unwrap()),
                    redirect_uris: vec![Url::from_str("https://example.com/callback").unwrap()],
                    scope: Some(CowStr::new_static("atproto")),
                    grant_types: Some(vec![CowStr::new_static("authorization_code")]),
                    token_endpoint_auth_method: Some(AuthMethod::PrivateKeyJwt.into()),
                    dpop_bound_access_tokens: Some(true),
                    jwks_uri: None,
                    jwks: Some(keyset.public_jwks()),
                    token_endpoint_auth_signing_alg: Some(CowStr::new_static("ES256")),
                }
            );
        }
    }
}
