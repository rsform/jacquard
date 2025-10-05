use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use http::{Request, Response, header::InvalidHeaderValue};
use jacquard_common::{
    CowStr,
    http_client::HttpClient,
    session::{MemorySessionStore, SessionStore, SessionStoreError},
};
use jose_jwa::{Algorithm, Signing};
use jose_jwk::{Jwk, Key, crypto};
use p256::ecdsa::SigningKey;
use rand::{RngCore, SeedableRng};
use sha2::Digest;
use smol_str::{SmolStr, ToSmolStr};

use crate::jose::{
    create_signed_jwt,
    jws::RegisteredHeader,
    jwt::{Claims, PublicClaims, RegisteredClaims},
};

pub const JWT_HEADER_TYP_DPOP: &str = "dpop+jwt";

#[derive(serde::Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(thiserror::Error, Debug, miette::Diagnostic)]
pub enum Error {
    #[error(transparent)]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error(transparent)]
    SessionStore(#[from] SessionStoreError),
    #[error("crypto error: {0:?}")]
    JwkCrypto(crypto::Error),
    #[error("key does not match any alg supported by the server")]
    UnsupportedKey,
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error("Inner: {0}")]
    Inner(#[source] Box<dyn std::error::Error + Send + Sync>),
}

type Result<T> = core::result::Result<T, Error>;

#[inline]
pub(crate) fn generate_jti() -> CowStr<'static> {
    let mut rng = rand::rngs::SmallRng::from_entropy();
    let mut bytes = [0u8; 12];
    rng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes).into()
}

/// Build a compact JWS (ES256) for DPoP with embedded public JWK.
#[inline]
pub fn build_dpop_proof<'s>(
    key: &Key,
    method: CowStr<'s>,
    url: CowStr<'s>,
    nonce: Option<CowStr<'s>>,
    ath: Option<CowStr<'s>>,
) -> Result<CowStr<'s>> {
    let secret = match crypto::Key::try_from(key).map_err(Error::JwkCrypto)? {
        crypto::Key::P256(crypto::Kind::Secret(sk)) => sk,
        _ => return Err(Error::UnsupportedKey),
    };
    build_dpop_proof_with_secret(&secret, method, url, nonce, ath)
}

/// Same as build_dpop_proof but takes a parsed secret key to avoid JSON roundtrips.
#[inline]
pub fn build_dpop_proof_with_secret<'s>(
    secret: &p256::SecretKey,
    method: CowStr<'s>,
    url: CowStr<'s>,
    nonce: Option<CowStr<'s>>,
    ath: Option<CowStr<'s>>,
) -> Result<CowStr<'s>> {
    let mut header = RegisteredHeader::from(Algorithm::Signing(Signing::Es256));
    header.typ = Some(JWT_HEADER_TYP_DPOP.into());
    header.jwk = Some(Jwk {
        key: Key::from(&crypto::Key::from(secret.public_key())),
        prm: Default::default(),
    });

    let claims = Claims {
        registered: RegisteredClaims {
            jti: Some(generate_jti()),
            iat: Some(Utc::now().timestamp()),
            ..Default::default()
        },
        public: PublicClaims {
            htm: Some(method),
            htu: Some(url),
            ath: ath,
            nonce: nonce,
        },
    };
    Ok(create_signed_jwt(
        SigningKey::from(secret.clone()),
        header.into(),
        claims,
    )?)
}

pub struct DpopClient<T, S = MemorySessionStore<CowStr<'static>, CowStr<'static>>>
where
    S: SessionStore<CowStr<'static>, CowStr<'static>>,
{
    inner: T,
    pub(crate) key: Key,
    nonces: S,
    is_auth_server: bool,
}

impl<T> DpopClient<T> {
    pub fn new(
        key: Key,
        http_client: T,
        is_auth_server: bool,
        supported_algs: &Option<Vec<CowStr<'static>>>,
    ) -> Result<Self> {
        if let Some(algs) = supported_algs {
            let alg = CowStr::from(match &key {
                Key::Ec(ec) => match &ec.crv {
                    jose_jwk::EcCurves::P256 => "ES256",
                    _ => unimplemented!(),
                },
                _ => unimplemented!(),
            });
            if !algs.contains(&alg) {
                return Err(Error::UnsupportedKey);
            }
        }
        let nonces = MemorySessionStore::<CowStr<'static>, CowStr<'static>>::default();
        Ok(Self {
            inner: http_client,
            key,
            nonces,
            is_auth_server,
        })
    }
}

impl<T, S> DpopClient<T, S>
where
    S: SessionStore<CowStr<'static>, CowStr<'static>>,
{
    fn build_proof<'s>(
        &self,
        method: CowStr<'s>,
        url: CowStr<'s>,
        ath: Option<CowStr<'s>>,
        nonce: Option<CowStr<'s>>,
    ) -> Result<CowStr<'s>> {
        build_dpop_proof(&self.key, method, url, nonce, ath)
    }
    fn is_use_dpop_nonce_error(&self, response: &http::Response<Vec<u8>>) -> bool {
        // https://datatracker.ietf.org/doc/html/rfc9449#name-authorization-server-provid
        if self.is_auth_server {
            if response.status() == 400 {
                if let Ok(res) = serde_json::from_slice::<ErrorResponse>(response.body()) {
                    return res.error == "use_dpop_nonce";
                };
            }
        }
        // https://datatracker.ietf.org/doc/html/rfc6750#section-3
        // https://datatracker.ietf.org/doc/html/rfc9449#name-resource-server-provided-no
        else if response.status() == 401 {
            if let Some(www_auth) = response
                .headers()
                .get("WWW-Authenticate")
                .and_then(|v| v.to_str().ok())
            {
                return www_auth.starts_with("DPoP")
                    && www_auth.contains(r#"error="use_dpop_nonce""#);
            }
        }
        false
    }
}

impl<T, S> HttpClient for DpopClient<T, S>
where
    T: HttpClient + Send + Sync + 'static,
    S: SessionStore<CowStr<'static>, CowStr<'static>> + Send + Sync + 'static,
{
    type Error = Error;

    async fn send_http(
        &self,
        mut request: Request<Vec<u8>>,
    ) -> core::result::Result<Response<Vec<u8>>, Self::Error> {
        let uri = request.uri();
        let nonce_key = CowStr::Owned(uri.authority().unwrap().to_smolstr());
        let method = CowStr::Owned(request.method().to_smolstr());
        let uri = CowStr::Owned(uri.to_smolstr());
        // https://datatracker.ietf.org/doc/html/rfc9449#section-4.2
        let ath = request
            .headers()
            .get("Authorization")
            .filter(|v| v.to_str().is_ok_and(|s| s.starts_with("DPoP ")))
            .map(|auth| {
                URL_SAFE_NO_PAD
                    .encode(sha2::Sha256::digest(&auth.as_bytes()[5..]))
                    .into()
            });

        let init_nonce = self.nonces.get(&nonce_key).await;
        let init_proof =
            self.build_proof(method.clone(), uri.clone(), ath.clone(), init_nonce.clone())?;
        request.headers_mut().insert("DPoP", init_proof.parse()?);
        let response = self
            .inner
            .send_http(request.clone())
            .await
            .map_err(|e| Error::Inner(e.into()))?;

        let next_nonce = response
            .headers()
            .get("DPoP-Nonce")
            .and_then(|v| v.to_str().ok())
            .map(|c| CowStr::Owned(SmolStr::new(c)));
        match &next_nonce {
            Some(s) if next_nonce != init_nonce => {
                // Store the fresh nonce for future requests
                self.nonces.set(nonce_key, s.clone()).await?;
            }
            _ => {
                // No nonce was returned or it is the same as the one we sent. No need to
                // update the nonce store, or retry the request.
                return Ok(response);
            }
        }

        if !self.is_use_dpop_nonce_error(&response) {
            return Ok(response);
        }
        let next_proof = self.build_proof(method, uri, ath, next_nonce)?;
        request.headers_mut().insert("DPoP", next_proof.parse()?);
        let response = self
            .inner
            .send_http(request)
            .await
            .map_err(|e| Error::Inner(e.into()))?;
        Ok(response)
    }
}

impl<T: Clone> Clone for DpopClient<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            key: self.key.clone(),
            nonces: self.nonces.clone(),
            is_auth_server: self.is_auth_server,
        }
    }
}
