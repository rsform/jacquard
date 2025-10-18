use std::future::Future;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use http::{Request, Response, header::InvalidHeaderValue};
use jacquard_common::{CowStr, IntoStatic, cowstr::ToCowStr, http_client::HttpClient};
use jacquard_identity::JacquardResolver;
use jose_jwa::{Algorithm, Signing};
use jose_jwk::{Jwk, Key, crypto};
use p256::ecdsa::SigningKey;
use rand::{RngCore, SeedableRng};
use sha2::Digest;

use crate::{
    jose::{
        create_signed_jwt,
        jws::RegisteredHeader,
        jwt::{Claims, PublicClaims, RegisteredClaims},
    },
    session::DpopDataSource,
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

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait DpopClient: HttpClient {
    fn dpop_server(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>>>;
    fn dpop_client(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>>>;
    fn wrap_request(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>>>;
}

pub trait DpopExt: HttpClient {
    fn dpop_server_call<'r, D>(&'r self, data_source: &'r mut D) -> DpopCall<'r, Self, D>
    where
        Self: Sized,
        D: DpopDataSource,
    {
        DpopCall::server(self, data_source)
    }

    fn dpop_call<'r, N>(&'r self, data_source: &'r mut N) -> DpopCall<'r, Self, N>
    where
        Self: Sized,
        N: DpopDataSource,
    {
        DpopCall::client(self, data_source)
    }
}

pub struct DpopCall<'r, C: HttpClient, D: DpopDataSource> {
    pub client: &'r C,
    pub is_to_auth_server: bool,
    pub data_source: &'r mut D,
}

impl<'r, C: HttpClient, N: DpopDataSource> DpopCall<'r, C, N> {
    pub fn server(client: &'r C, data_source: &'r mut N) -> Self {
        Self {
            client,
            is_to_auth_server: true,
            data_source,
        }
    }

    pub fn client(client: &'r C, data_source: &'r mut N) -> Self {
        Self {
            client,
            is_to_auth_server: false,
            data_source,
        }
    }

    pub async fn send(self, request: Request<Vec<u8>>) -> Result<Response<Vec<u8>>> {
        wrap_request_with_dpop(
            self.client,
            self.data_source,
            self.is_to_auth_server,
            request,
        )
        .await
    }

    #[cfg(feature = "streaming")]
    pub async fn send_streaming(
        self,
        request: Request<Vec<u8>>,
    ) -> Result<jacquard_common::xrpc::StreamingResponse>
    where
        C: jacquard_common::http_client::HttpClientExt,
    {
        wrap_request_with_dpop_streaming(
            self.client,
            self.data_source,
            self.is_to_auth_server,
            request,
        )
        .await
    }

    #[cfg(feature = "streaming")]
    pub async fn send_bidirectional(
        self,
        parts: http::request::Parts,
        body: jacquard_common::stream::ByteStream,
    ) -> Result<jacquard_common::xrpc::StreamingResponse>
    where
        C: jacquard_common::http_client::HttpClientExt,
    {
        wrap_request_with_dpop_bidirectional(
            self.client,
            self.data_source,
            self.is_to_auth_server,
            parts,
            body,
        )
        .await
    }
}

/// Extract authorization hash from request headers
fn extract_ath(headers: &http::HeaderMap) -> Option<CowStr<'static>> {
    headers
        .get("Authorization")
        .filter(|v| v.to_str().is_ok_and(|s| s.starts_with("DPoP ")))
        .map(|auth| {
            URL_SAFE_NO_PAD
                .encode(sha2::Sha256::digest(&auth.as_bytes()[5..]))
                .into()
        })
}

/// Get nonce from data source based on target
fn get_nonce<N: DpopDataSource>(data_source: &N, is_to_auth_server: bool) -> Option<CowStr<'_>> {
    if is_to_auth_server {
        data_source.authserver_nonce()
    } else {
        data_source.host_nonce()
    }
}

/// Store nonce in data source based on target
fn store_nonce<N: DpopDataSource>(
    data_source: &mut N,
    is_to_auth_server: bool,
    nonce: CowStr<'static>,
) {
    if is_to_auth_server {
        data_source.set_authserver_nonce(nonce);
    } else {
        data_source.set_host_nonce(nonce);
    }
}

pub async fn wrap_request_with_dpop<T, N>(
    client: &T,
    data_source: &mut N,
    is_to_auth_server: bool,
    mut request: Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>>
where
    T: HttpClient,
    N: DpopDataSource,
{
    let uri = request.uri().clone();
    let method = request.method().to_cowstr().into_static();
    let uri = uri.to_cowstr();
    let ath = extract_ath(request.headers());

    let init_nonce = get_nonce(data_source, is_to_auth_server);
    let init_proof = build_dpop_proof(
        data_source.key(),
        method.clone(),
        uri.clone(),
        init_nonce.clone(),
        ath.clone(),
    )?;
    request.headers_mut().insert("DPoP", init_proof.parse()?);
    let response = client
        .send_http(request.clone())
        .await
        .map_err(|e| Error::Inner(e.into()))?;

    let next_nonce = response
        .headers()
        .get("DPoP-Nonce")
        .and_then(|v| v.to_str().ok())
        .map(|c| CowStr::from(c.to_string()));
    match &next_nonce {
        Some(s) if next_nonce != init_nonce => {
            store_nonce(data_source, is_to_auth_server, s.clone());
        }
        _ => {
            return Ok(response);
        }
    }

    if !is_use_dpop_nonce_error(is_to_auth_server, &response) {
        return Ok(response);
    }
    let next_proof = build_dpop_proof(data_source.key(), method, uri, next_nonce, ath)?;
    request.headers_mut().insert("DPoP", next_proof.parse()?);
    let response = client
        .send_http(request)
        .await
        .map_err(|e| Error::Inner(e.into()))?;
    Ok(response)
}

#[cfg(feature = "streaming")]
pub async fn wrap_request_with_dpop_streaming<T, N>(
    client: &T,
    data_source: &mut N,
    is_to_auth_server: bool,
    mut request: Request<Vec<u8>>,
) -> Result<jacquard_common::xrpc::StreamingResponse>
where
    T: jacquard_common::http_client::HttpClientExt,
    N: DpopDataSource,
{
    use jacquard_common::xrpc::StreamingResponse;

    let uri = request.uri().clone();
    let method = request.method().to_cowstr().into_static();
    let uri = uri.to_cowstr();
    let ath = extract_ath(request.headers());

    let init_nonce = get_nonce(data_source, is_to_auth_server);
    let init_proof = build_dpop_proof(
        data_source.key(),
        method.clone(),
        uri.clone(),
        init_nonce.clone(),
        ath.clone(),
    )?;
    request.headers_mut().insert("DPoP", init_proof.parse()?);
    let http_response = client
        .send_http_streaming(request.clone())
        .await
        .map_err(|e| Error::Inner(e.into()))?;

    let (parts, body) = http_response.into_parts();
    let next_nonce = parts
        .headers
        .get("DPoP-Nonce")
        .and_then(|v| v.to_str().ok())
        .map(|c| CowStr::from(c.to_string()));
    match &next_nonce {
        Some(s) if next_nonce != init_nonce => {
            store_nonce(data_source, is_to_auth_server, s.clone());
        }
        _ => {
            return Ok(StreamingResponse::new(parts, body));
        }
    }

    // For streaming responses, we can't easily check the body for use_dpop_nonce error
    // We check status code + headers only
    if !is_use_dpop_nonce_error_streaming(is_to_auth_server, parts.status, &parts.headers) {
        return Ok(StreamingResponse::new(parts, body));
    }

    let next_proof = build_dpop_proof(data_source.key(), method, uri, next_nonce, ath)?;
    request.headers_mut().insert("DPoP", next_proof.parse()?);
    let http_response = client
        .send_http_streaming(request)
        .await
        .map_err(|e| Error::Inner(e.into()))?;
    let (parts, body) = http_response.into_parts();
    Ok(StreamingResponse::new(parts, body))
}

#[cfg(feature = "streaming")]
pub async fn wrap_request_with_dpop_bidirectional<T, N>(
    client: &T,
    data_source: &mut N,
    is_to_auth_server: bool,
    mut parts: http::request::Parts,
    body: jacquard_common::stream::ByteStream,
) -> Result<jacquard_common::xrpc::StreamingResponse>
where
    T: jacquard_common::http_client::HttpClientExt,
    N: DpopDataSource,
{
    use jacquard_common::xrpc::StreamingResponse;

    let uri = parts.uri.clone();
    let method = parts.method.to_cowstr().into_static();
    let uri = uri.to_cowstr();
    let ath = extract_ath(&parts.headers);

    let init_nonce = get_nonce(data_source, is_to_auth_server);
    let init_proof = build_dpop_proof(
        data_source.key(),
        method.clone(),
        uri.clone(),
        init_nonce.clone(),
        ath.clone(),
    )?;
    parts.headers.insert("DPoP", init_proof.parse()?);

    // Clone the stream for potential retry
    let (body1, body2) = body.tee();

    let http_response = client
        .send_http_bidirectional(parts.clone(), body1.into_inner())
        .await
        .map_err(|e| Error::Inner(e.into()))?;

    let (resp_parts, resp_body) = http_response.into_parts();
    let next_nonce = resp_parts
        .headers
        .get("DPoP-Nonce")
        .and_then(|v| v.to_str().ok())
        .map(|c| CowStr::from(c.to_string()));
    match &next_nonce {
        Some(s) if next_nonce != init_nonce => {
            store_nonce(data_source, is_to_auth_server, s.clone());
        }
        _ => {
            return Ok(StreamingResponse::new(resp_parts, resp_body));
        }
    }

    // For streaming responses, we can't easily check the body for use_dpop_nonce error
    // We check status code + headers only
    if !is_use_dpop_nonce_error_streaming(is_to_auth_server, resp_parts.status, &resp_parts.headers)
    {
        return Ok(StreamingResponse::new(resp_parts, resp_body));
    }

    let next_proof = build_dpop_proof(data_source.key(), method, uri, next_nonce, ath)?;
    parts.headers.insert("DPoP", next_proof.parse()?);
    let http_response = client
        .send_http_bidirectional(parts, body2.into_inner())
        .await
        .map_err(|e| Error::Inner(e.into()))?;
    let (parts, body) = http_response.into_parts();
    Ok(StreamingResponse::new(parts, body))
}

#[cfg(feature = "streaming")]
fn is_use_dpop_nonce_error_streaming(
    is_to_auth_server: bool,
    status: http::StatusCode,
    headers: &http::HeaderMap,
) -> bool {
    if is_to_auth_server && status == 400 {
        // Can't check body for streaming, so we rely on DPoP-Nonce header presence
        return false;
    }
    if !is_to_auth_server && status == 401 {
        if let Some(www_auth) = headers
            .get("WWW-Authenticate")
            .and_then(|v| v.to_str().ok())
        {
            return www_auth.starts_with("DPoP") && www_auth.contains(r#"error="use_dpop_nonce""#);
        }
    }
    false
}

#[inline]
fn is_use_dpop_nonce_error(is_to_auth_server: bool, response: &Response<Vec<u8>>) -> bool {
    // https://datatracker.ietf.org/doc/html/rfc9449#name-authorization-server-provid
    if is_to_auth_server {
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
            return www_auth.starts_with("DPoP") && www_auth.contains(r#"error="use_dpop_nonce""#);
        }
    }
    false
}

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

impl DpopExt for JacquardResolver {}
