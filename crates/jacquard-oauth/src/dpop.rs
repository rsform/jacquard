use std::error::Error as StdError;
use std::fmt;
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
use smol_str::SmolStr;

use crate::{
    jose::{
        jws::RegisteredHeader,
        jwt::{Claims, PublicClaims, RegisteredClaims},
        signing,
    },
    session::DpopDataSource,
};

/// The `typ` header value required in all DPoP proof JWTs, per RFC 9449.
pub const JWT_HEADER_TYP_DPOP: &str = "dpop+jwt";

#[derive(serde::Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Boxed error type for error sources.
pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// Target server type for DPoP requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpopTarget {
    /// OAuth authorization server (token endpoint, PAR, etc.)
    AuthServer,
    /// Resource server (PDS, AppView, etc.)
    ResourceServer,
}

impl fmt::Display for DpopTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DpopTarget::AuthServer => write!(f, "auth server"),
            DpopTarget::ResourceServer => write!(f, "resource server"),
        }
    }
}

/// Error categories for DPoP operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DpopErrorKind {
    /// DPoP proof construction failed.
    ProofBuild,
    /// Initial HTTP request failed.
    Transport,
    /// Retry after nonce update also failed.
    NonceRetry,
    /// Header value parsing failed.
    InvalidHeader,
    /// JWK crypto operation failed.
    Crypto,
    /// Key type not supported for DPoP.
    UnsupportedKey,
    /// JSON serialization failed.
    Serialization,
}

impl fmt::Display for DpopErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DpopErrorKind::ProofBuild => write!(f, "DPoP proof construction failed"),
            DpopErrorKind::Transport => write!(f, "HTTP request failed"),
            DpopErrorKind::NonceRetry => write!(f, "request failed after nonce retry"),
            DpopErrorKind::InvalidHeader => write!(f, "invalid header value"),
            DpopErrorKind::Crypto => write!(f, "JWK crypto operation failed"),
            DpopErrorKind::UnsupportedKey => write!(f, "unsupported key type"),
            DpopErrorKind::Serialization => write!(f, "JSON serialization failed"),
        }
    }
}

/// DPoP operation error with rich context.
#[derive(Debug, miette::Diagnostic)]
pub struct DpopError {
    kind: DpopErrorKind,
    target: Option<DpopTarget>,
    url: Option<SmolStr>,
    source: Option<BoxError>,
    context: Option<SmolStr>,
    #[help]
    help: Option<&'static str>,
}

impl fmt::Display for DpopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;

        if let Some(target) = &self.target {
            write!(f, " (to {})", target)?;
        }

        if let Some(url) = &self.url {
            write!(f, " [{}]", url)?;
        }

        if let Some(ctx) = &self.context {
            write!(f, ": {}", ctx)?;
        }

        Ok(())
    }
}

impl StdError for DpopError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn StdError + 'static))
    }
}

impl DpopError {
    /// Create a new error with the given kind.
    fn new(kind: DpopErrorKind) -> Self {
        Self {
            kind,
            target: None,
            url: None,
            source: None,
            context: None,
            help: None,
        }
    }

    /// Get the error kind.
    pub fn kind(&self) -> DpopErrorKind {
        self.kind
    }

    /// Get the target server type, if known.
    pub fn target(&self) -> Option<DpopTarget> {
        self.target
    }

    /// Get the URL, if known.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Get the context string, if any.
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    // Builder methods

    fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    fn with_target(mut self, target: DpopTarget) -> Self {
        self.target = Some(target);
        self
    }

    fn with_url(mut self, url: impl Into<SmolStr>) -> Self {
        self.url = Some(url.into());
        self
    }

    fn with_help(mut self, help: &'static str) -> Self {
        self.help = Some(help);
        self
    }

    /// Add context information to the error.
    pub fn with_context(mut self, context: impl Into<SmolStr>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Append additional context to the error.
    pub fn append_context(mut self, additional: impl AsRef<str>) -> Self {
        self.context = Some(match self.context.take() {
            Some(existing) => smol_str::format_smolstr!("{}: {}", existing, additional.as_ref()),
            None => SmolStr::new(additional.as_ref()),
        });
        self
    }

    /// Add NSID context (for use by higher-level code).
    pub fn for_nsid(self, nsid: &str) -> Self {
        self.append_context(smol_str::format_smolstr!("[{}]", nsid))
    }

    // Constructors for specific error kinds

    /// Create a proof build error.
    pub fn proof_build(source: impl StdError + Send + Sync + 'static) -> Self {
        Self::new(DpopErrorKind::ProofBuild)
            .with_source(source)
            .with_help("check that the DPoP key is valid and the JWT claims are correct")
    }

    /// Create a transport error for initial request.
    pub fn transport(
        target: DpopTarget,
        url: impl Into<SmolStr>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self::new(DpopErrorKind::Transport)
            .with_target(target)
            .with_url(url)
            .with_source(source)
    }

    /// Create a nonce retry error.
    pub fn nonce_retry(
        target: DpopTarget,
        url: impl Into<SmolStr>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self::new(DpopErrorKind::NonceRetry)
            .with_target(target)
            .with_url(url)
            .with_source(source)
            .with_help(
                "the server rejected both the initial request and the retry with updated nonce",
            )
    }

    /// Create an invalid header error.
    pub fn invalid_header(source: InvalidHeaderValue) -> Self {
        Self::new(DpopErrorKind::InvalidHeader)
            .with_source(source)
            .with_help("the DPoP proof could not be set as a header value")
    }

    /// Create a crypto error.
    pub fn crypto(source: crypto::Error) -> Self {
        Self::new(DpopErrorKind::Crypto)
            .with_context(format!("{:?}", source))
            .with_help(
                "ensure the key is a valid secret key in JWK format with a supported algorithm",
            )
    }

    /// Create an unsupported key error.
    pub fn unsupported_key() -> Self {
        Self::new(DpopErrorKind::UnsupportedKey)
            .with_help("DPoP requires an EC P-256 key; other key types are not currently supported")
    }

    /// Create a serialization error.
    pub fn serialization(source: serde_json::Error) -> Self {
        Self::new(DpopErrorKind::Serialization)
            .with_source(source)
            .with_help("failed to serialize JWT claims or header")
    }
}

impl From<InvalidHeaderValue> for DpopError {
    fn from(e: InvalidHeaderValue) -> Self {
        Self::invalid_header(e)
    }
}

impl From<serde_json::Error> for DpopError {
    fn from(e: serde_json::Error) -> Self {
        Self::serialization(e)
    }
}

impl From<DpopError> for jacquard_common::error::ClientError {
    fn from(e: DpopError) -> Self {
        use jacquard_common::error::{AuthError, ClientError};

        // Extract context from DpopError before converting
        let kind = e.kind;
        let url = e.url.clone();
        let context = e.context.clone();
        let target = e.target;

        // Build combined context string
        let combined_context = match (target, context) {
            (Some(t), Some(c)) => Some(smol_str::format_smolstr!("to {}: {}", t, c)),
            (Some(t), None) => Some(smol_str::format_smolstr!("to {}", t)),
            (None, Some(c)) => Some(c),
            (None, None) => None,
        };

        // Map DpopErrorKind to appropriate ClientError
        let mut client_err = match kind {
            DpopErrorKind::ProofBuild | DpopErrorKind::Crypto | DpopErrorKind::UnsupportedKey => {
                ClientError::auth(AuthError::DpopProofFailed)
            }
            DpopErrorKind::NonceRetry => ClientError::auth(AuthError::DpopNonceFailed),
            DpopErrorKind::Transport => ClientError::new(
                jacquard_common::error::ClientErrorKind::Transport,
                Some(Box::new(e)),
            ),
            DpopErrorKind::InvalidHeader | DpopErrorKind::Serialization => {
                let msg = smol_str::format_smolstr!("DPoP: {:?}", kind);
                ClientError::encode(msg)
            }
        };

        // Add URL if present (skip for Transport since e was consumed)
        if !matches!(kind, DpopErrorKind::Transport) {
            if let Some(u) = url {
                client_err = client_err.with_url(u);
            }
        }

        // Add combined context if present (skip for Transport since e was consumed)
        if !matches!(kind, DpopErrorKind::Transport) {
            if let Some(ctx) = combined_context {
                client_err = client_err.with_context(ctx);
            }
        }

        client_err
    }
}

type Result<T> = core::result::Result<T, DpopError>;

/// An HTTP client capable of making DPoP-protected requests to both auth servers and resource servers.
///
/// Implementors must be able to attach a DPoP proof header, handle nonce challenges, and
/// retry transparently on `use_dpop_nonce` errors.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait DpopClient: HttpClient {
    /// Send a DPoP-protected request to an authorization server (token endpoint, PAR, etc.).
    fn dpop_server(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>>>;
    /// Send a DPoP-protected request to a resource server (PDS, AppView, etc.).
    fn dpop_client(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>>>;
    /// Send a DPoP-protected request, inferring the target type from the request context.
    fn wrap_request(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>>>;
}

/// Extension trait for any [`HttpClient`] that adds builder methods for constructing
/// DPoP-protected request calls without requiring a full [`DpopClient`] implementation.
pub trait DpopExt: HttpClient {
    /// Begin building a DPoP-protected request targeting an authorization server.
    fn dpop_server_call<'r, D>(&'r self, data_source: &'r mut D) -> DpopCall<'r, Self, D>
    where
        Self: Sized,
        D: DpopDataSource,
    {
        DpopCall::server(self, data_source)
    }

    /// Begin building a DPoP-protected request targeting a resource server.
    fn dpop_call<'r, N>(&'r self, data_source: &'r mut N) -> DpopCall<'r, Self, N>
    where
        Self: Sized,
        N: DpopDataSource,
    {
        DpopCall::client(self, data_source)
    }
}

/// A builder for a single DPoP-protected HTTP request, holding references to the underlying
/// client and the session data source that supplies nonces and the DPoP signing key.
pub struct DpopCall<'r, C: HttpClient, D: DpopDataSource> {
    /// The HTTP client that will send the request.
    pub client: &'r C,
    /// Whether the request targets an authorization server rather than a resource server.
    ///
    /// This controls which nonce slot is read from and written to, and how `use_dpop_nonce`
    /// errors are detected in the response.
    pub is_to_auth_server: bool,
    /// The session data source providing the DPoP key and current nonces.
    pub data_source: &'r mut D,
}

impl<'r, C: HttpClient, N: DpopDataSource> DpopCall<'r, C, N> {
    /// Create a call builder targeting an authorization server.
    pub fn server(client: &'r C, data_source: &'r mut N) -> Self {
        Self {
            client,
            is_to_auth_server: true,
            data_source,
        }
    }

    /// Create a call builder targeting a resource server.
    pub fn client(client: &'r C, data_source: &'r mut N) -> Self {
        Self {
            client,
            is_to_auth_server: false,
            data_source,
        }
    }

    /// Send the request with a DPoP proof, retrying once if the server provides a new nonce.
    pub async fn send(self, request: Request<Vec<u8>>) -> Result<Response<Vec<u8>>> {
        wrap_request_with_dpop(
            self.client,
            self.data_source,
            self.is_to_auth_server,
            request,
        )
        .await
    }

    /// Sends the request with DPoP proof and returns a streaming response.
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

    /// Sends the request with DPoP proof using bidirectional streaming.
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
        .get("authorization")
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

/// Attach a DPoP proof to `request`, send it, and transparently retry once if the server
/// responds with a `use_dpop_nonce` error and a fresh nonce.
///
/// The nonce is read from and written back to `data_source` based on `is_to_auth_server`,
/// keeping the two nonce slots (auth server vs. resource server) independent.
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
    let target = if is_to_auth_server {
        DpopTarget::AuthServer
    } else {
        DpopTarget::ResourceServer
    };
    let uri = request.uri().clone();
    let method = request.method().to_cowstr().into_static();
    let url_str: SmolStr = uri.to_cowstr().as_ref().into();
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
        .map_err(|e| DpopError::transport(target, url_str.clone(), e))?;

    let next_nonce = response
        .headers()
        .get("dpop-nonce")
        .and_then(|v| v.to_str().ok())
        .map(|c| CowStr::copy_from_str(c));
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
        .map_err(|e| DpopError::nonce_retry(target, url_str, e))?;
    Ok(response)
}

/// Wraps an HTTP request with a DPoP proof and returns a streaming response.
///
/// Like [`wrap_request_with_dpop`], but returns a [`StreamingResponse`](jacquard_common::xrpc::StreamingResponse)
/// instead of buffering the body. Nonce retry is limited to status/header inspection
/// since the body stream cannot be rewound.
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

    let target = if is_to_auth_server {
        DpopTarget::AuthServer
    } else {
        DpopTarget::ResourceServer
    };
    let uri = request.uri().clone();
    let method = request.method().to_cowstr().into_static();
    let url_str: SmolStr = uri.to_cowstr().as_ref().into();
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
        .map_err(|e| DpopError::transport(target, url_str.clone(), e))?;

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
        .map_err(|e| DpopError::nonce_retry(target, url_str, e))?;
    let (parts, body) = http_response.into_parts();
    Ok(StreamingResponse::new(parts, body))
}

/// Wraps an HTTP request with a DPoP proof using bidirectional streaming.
///
/// Similar to [`wrap_request_with_dpop_streaming`] but accepts a [`ByteStream`](jacquard_common::stream::ByteStream)
/// request body for upload streaming scenarios.
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

    let target = if is_to_auth_server {
        DpopTarget::AuthServer
    } else {
        DpopTarget::ResourceServer
    };
    let uri = parts.uri.clone();
    let method = parts.method.to_cowstr().into_static();
    let url_str: SmolStr = uri.to_cowstr().as_ref().into();
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
        .map_err(|e| DpopError::transport(target, url_str.clone(), e))?;

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
        .map_err(|e| DpopError::nonce_retry(target, url_str, e))?;
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
            .get("www-authenticate")
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
            .get("www-authenticate")
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
    let secret = match crypto::Key::try_from(key).map_err(DpopError::crypto)? {
        crypto::Key::P256(crypto::Kind::Secret(sk)) => sk,
        _ => return Err(DpopError::unsupported_key()),
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
    Ok(signing::create_signed_jwt_es256(
        SigningKey::from(secret.clone()),
        header.into(),
        claims,
    )?)
}

impl DpopExt for JacquardResolver {}
