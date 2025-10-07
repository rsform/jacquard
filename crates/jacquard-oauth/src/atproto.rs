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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtprotoClientMetadata<'m> {
    pub client_id: Url,
    pub client_uri: Option<Url>,
    pub redirect_uris: Vec<Url>,
    pub grant_types: Vec<GrantType>,
    pub scopes: Vec<Scope<'m>>,
    pub jwks_uri: Option<Url>,
}

impl<'m> AtprotoClientMetadata<'m> {
    pub fn new(
        client_id: Url,
        client_uri: Option<Url>,
        redirect_uris: Vec<Url>,
        grant_types: Vec<GrantType>,
        scopes: Vec<Scope<'m>>,
        jwks_uri: Option<Url>,
    ) -> Self {
        Self {
            client_id,
            client_uri,
            redirect_uris,
            grant_types,
            scopes,
            jwks_uri,
        }
    }

    pub fn new_localhost(
        mut redirect_uris: Option<Vec<Url>>,
        scopes: Option<Vec<Scope<'m>>>,
    ) -> Self {
        // coerce redirect uris to localhost
        if let Some(redirect_uris) = &mut redirect_uris {
            for redirect_uri in redirect_uris {
                redirect_uri.set_host(Some("http://localhost")).unwrap();
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
            scope: scopes
                .as_ref()
                .map(|s| Scope::serialize_multiple(s.as_slice())),
        })
        .ok();
        let mut client_id = String::from("http://localhost");
        if let Some(query) = query
            && !query.is_empty()
        {
            client_id.push_str(&format!("?{query}"));
        }
        Self {
            client_id: Url::parse(&client_id).unwrap(),
            client_uri: None,
            redirect_uris: redirect_uris.unwrap_or(vec![
                Url::from_str("http://127.0.0.1/").unwrap(),
                Url::from_str("http://[::1]/").unwrap(),
            ]),
            grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
            scopes: scopes.unwrap_or(vec![Scope::Atproto]),
            jwks_uri: None,
        }
    }
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

    Ok(OAuthClientMetadata {
        client_id: metadata.client_id,
        client_uri: metadata.client_uri,
        redirect_uris: metadata.redirect_uris,
        token_endpoint_auth_method: Some(auth_method.into()),
        grant_types: Some(metadata.grant_types.into_iter().map(|v| v.into()).collect()),
        scope: Some(Scope::serialize_multiple(metadata.scopes.as_slice())),
        dpop_bound_access_tokens: Some(true),
        jwks_uri,
        jwks,
        token_endpoint_auth_signing_alg: if keyset.is_some() {
            Some(CowStr::new_static("ES256"))
        } else {
            None
        },
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
            atproto_client_metadata(AtprotoClientMetadata::new_localhost(None, None), &None)
                .unwrap(),
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
            atproto_client_metadata(AtprotoClientMetadata::new_localhost(
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
                )
            ), &None)
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
            let err = atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![Url::from_str("https://127.0.0.1/").unwrap()]),
                    None,
                ),
                &None,
            )
            .expect_err("expected to fail");
            assert!(matches!(
                err,
                Error::LocalhostClient(LocalhostClientError::NotHttpScheme)
            ));
        }
        {
            let err = atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![Url::from_str("http://localhost:8000/").unwrap()]),
                    None,
                ),
                &None,
            )
            .expect_err("expected to fail");
            assert!(matches!(
                err,
                Error::LocalhostClient(LocalhostClientError::Localhost)
            ));
        }
        {
            let err = atproto_client_metadata(
                AtprotoClientMetadata::new_localhost(
                    Some(vec![Url::from_str("http://192.168.0.0/").unwrap()]),
                    None,
                ),
                &None,
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
            grant_types: vec![GrantType::AuthorizationCode],
            scopes: vec![Scope::Atproto],
            jwks_uri: None,
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
