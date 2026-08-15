//! # Stateless XRPC utilities and request/response mapping
//!
//! Mapping overview:
//! - Success (2xx): parse body into the endpoint's typed output.
//! - 400: try typed error; on failure, fall back to a generic XRPC error (with
//!   `nsid`, `method`, and `http_status`) and map common auth errors.
//! - 401: if `WWW-Authenticate` is present, return
//!   `ClientError::Auth(AuthError::Other(header))` so higher layers (OAuth/DPoP)
//!   can inspect `error="invalid_token"` or `error="use_dpop_nonce"` and refresh/retry.
//!   If the header is absent, parse the body and map auth errors to
//!   `AuthError::TokenExpired`/`InvalidToken`.

#[cfg(feature = "streaming")]
pub mod streaming;

/// Hand-written XRPC types for com.atproto endpoints (bootstrap types).
pub mod atproto;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use ipld_core::ipld::Ipld;
#[cfg(feature = "streaming")]
pub use streaming::{
    StreamingResponse, XrpcProcedureSend, XrpcProcedureStream, XrpcResponseStream, XrpcStreamResp,
};

#[cfg(feature = "websocket")]
pub mod subscription;

#[cfg(feature = "streaming")]
use crate::StreamError;
use crate::bos::BosStr;
use crate::error::DecodeError;
use crate::http_client::HttpClient;
#[cfg(feature = "streaming")]
use crate::http_client::HttpClientExt;
use crate::types::value::Data;
use crate::{AuthorizationToken, error::AuthError};
use crate::{BorrowOrShare, DefaultStr};
use crate::{CowStr, error::XrpcResult};
use crate::{IntoStatic, types::value::RawData};
use bytes::Bytes;
use core::error::Error;
use core::fmt::{self, Debug};
use core::marker::PhantomData;
use http::{
    HeaderName, HeaderValue, Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::deps::fluent_uri::Uri;
#[cfg(feature = "websocket")]
pub use subscription::{
    BasicSubscriptionClient, MessageEncoding, SubscriptionCall, SubscriptionClient,
    SubscriptionEndpoint, SubscriptionExt, SubscriptionOptions, SubscriptionResp,
    SubscriptionStream, TungsteniteSubscriptionClient, XrpcSubscription,
};

/// Normalize a base URI by removing trailing slashes.
///
/// This is useful for XRPC clients where the base URI might be provided with
/// a trailing slash (e.g., "<https://bsky.social/>") but needs to be normalized
/// for consistent path building. Since trimming a trailing slash from a valid URI
/// always yields a valid URI, the result is guaranteed to be valid.
pub fn normalize_base_uri(uri: Uri<String>) -> Uri<String> {
    let s = uri.as_str();
    if s.ends_with('/') && s.len() > 1 {
        let trimmed = s.trim_end_matches('/');
        // Invariant: trimming trailing slashes from a valid URI always yields a valid URI.
        Uri::parse(trimmed.to_string())
            .expect("trimming trailing slash from valid URI yields valid URI")
    } else {
        uri
    }
}

/// Error type for encoding XRPC requests
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(miette::Diagnostic))]
#[non_exhaustive]
pub enum EncodeError {
    /// Failed to serialize query parameters
    #[error("Failed to serialize query: {0}")]
    Query(
        #[from]
        #[source]
        serde_html_form::ser::Error,
    ),
    /// Failed to serialize JSON body
    #[error("Failed to serialize JSON: {0}")]
    Json(
        #[from]
        #[source]
        serde_json::Error,
    ),
    /// Other encoding error
    #[error("Encoding error: {0}")]
    Other(String),
}

/// XRPC method type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrpcMethod {
    /// Query (HTTP GET)
    Query,
    /// Procedure (HTTP POST)
    Procedure(&'static str),
}

impl XrpcMethod {
    /// Get the HTTP method string
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Query => "GET",
            Self::Procedure(_) => "POST",
        }
    }

    /// Get the body encoding type for this method (procedures only)
    pub const fn body_encoding(&self) -> Option<&'static str> {
        match self {
            Self::Query => None,
            Self::Procedure(enc) => Some(enc),
        }
    }
}

/// Trait for XRPC request types (queries and procedures)
///
/// This trait provides metadata about XRPC endpoints including the NSID,
/// HTTP method, encoding, and associated output type.
///
/// The trait is implemented on the request parameters/input type itself.
pub trait XrpcRequest {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;

    /// Response type returned from the XRPC call (marker struct)
    type Response: XrpcResp;

    /// Encode the request body for procedures.
    ///
    /// Default implementation serializes to JSON. Override for non-JSON encodings.
    fn encode_body(&self, buffer: &mut Vec<u8>) -> Result<(), EncodeError>
    where
        Self: Serialize,
    {
        Ok(serde_json::to_writer(buffer, self)?)
    }

    /// Decode the request body for procedures.
    ///
    /// Default implementation deserializes from JSON. Override for non-JSON encodings.
    fn decode_body<'de>(body: &'de [u8]) -> Result<Self, DecodeError>
    where
        Self: Deserialize<'de>,
    {
        let body: Self = serde_json::from_slice(body)?;

        Ok(body)
    }
}

/// Trait for XRPC Response types
///
/// It mirrors the NSID and carries the encoding types as well as Output (success) and Err types.
///
/// `Output` is parameterised on a backing string type `S: Bos<str>`, allowing callers to choose
/// between zero-copy (`CowStr<'_>`), small-string-optimised (`SmolStr`), or other backing types.
///
/// `Err` is a plain associated type (not a GAT) — error types are always `SmolStr`-backed and
/// `DeserializeOwned`. This keeps error handling simple and avoids lifetime gymnastics on the
/// unhappy path.
pub trait XrpcResp {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// Output encoding (MIME type)
    const ENCODING: &'static str;

    /// Response output type, parameterised on backing string type.
    type Output<S: BosStr>;

    /// Error type for this request. Always owned (`DeserializeOwned`).
    type Err: Error + Serialize + DeserializeOwned;

    /// Encode the response output body.
    ///
    /// Default implementation serializes to JSON. Override for non-JSON encodings.
    fn encode_output<S: BosStr>(output: &Self::Output<S>) -> Result<Vec<u8>, EncodeError>
    where
        Self::Output<S>: Serialize,
    {
        Ok(serde_json::to_vec(output)?)
    }

    /// Decode the response output body.
    ///
    /// Default implementation deserializes from JSON. Override for non-JSON encodings.
    fn decode_output<'de, S>(body: &'de [u8]) -> core::result::Result<Self::Output<S>, DecodeError>
    where
        S: BosStr + Deserialize<'de>,
        Self::Output<S>: Deserialize<'de>,
    {
        let body = serde_json::from_slice(body).map_err(DecodeError::Json)?;
        Ok(body)
    }
}

/// XRPC server endpoint trait
///
/// Defines the fully-qualified path and method, as well as request and response types.
/// The `Request` associated type is parameterised on `S` so codegen doesn't need to pick
/// a backing string type — the server picks it at the call site.
///
/// It is implemented by the code generation on a marker struct, like the client-side [XrpcResp] trait.
pub trait XrpcEndpoint {
    /// Fully-qualified path ('/xrpc/\[nsid\]') where this endpoint should live on the server
    const PATH: &'static str;
    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;
    /// XRPC Request data type
    type Request<S: BosStr>: XrpcRequest;
    /// XRPC Response data type
    type Response: XrpcResp;
}

/// Error type for XRPC endpoints that don't define any errors.
///
/// Always `SmolStr`-backed and owned — error types don't need zero-copy deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GenericError(Data<SmolStr>);

impl fmt::Display for GenericError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for GenericError {}

/// Per-request options for XRPC calls.
#[derive(Debug, Clone)]
pub struct CallOptions<S: BosStr = DefaultStr> {
    /// Optional Authorization to apply (`Bearer` or `DPoP`).
    pub auth: Option<AuthorizationToken<S>>,
    /// `atproto-proxy` header value.
    pub atproto_proxy: Option<S>,
    /// `atproto-accept-labelers` header values.
    pub atproto_accept_labelers: Option<Vec<S>>,
    /// Extra headers to attach to this request.
    pub extra_headers: Vec<(HeaderName, HeaderValue)>,
}

impl Default for CallOptions {
    fn default() -> Self {
        Self {
            auth: None,
            atproto_proxy: None,
            atproto_accept_labelers: None,
            extra_headers: Vec::new(),
        }
    }
}

impl<S: BosStr> CallOptions<S> {
    /// Borrows the fields of this struct as `&str` references.
    pub fn borrow(&self) -> CallOptions<&str> {
        CallOptions {
            auth: self.auth.as_ref().map(|auth| auth.borrow()),
            atproto_proxy: self
                .atproto_proxy
                .as_ref()
                .map(|proxy| proxy.borrow_or_share()),
            atproto_accept_labelers: self
                .atproto_accept_labelers
                .as_ref()
                .map(|labelers| labelers.iter().map(|l| l.as_ref()).collect()),
            extra_headers: self.extra_headers.clone(),
        }
    }
}

impl<S: BosStr + IntoStatic> IntoStatic for CallOptions<S>
where
    <S as IntoStatic>::Output: BosStr + 'static,
{
    type Output = CallOptions<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        CallOptions {
            auth: self.auth.map(|auth| auth.into_static()),
            atproto_proxy: self.atproto_proxy.map(|proxy| proxy.into_static()),
            atproto_accept_labelers: self
                .atproto_accept_labelers
                .map(|labelers| labelers.into_static()),
            extra_headers: self.extra_headers,
        }
    }
}

/// Extension for stateless XRPC calls on any `HttpClient`.
///
/// Example
/// ```no_run
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use jacquard_common::xrpc::XrpcExt;
/// use jacquard_common::http_client::HttpClient;
/// use jacquard_common::deps::fluent_uri::Uri;
///
/// let http = reqwest::Client::new();
/// let base = Uri::parse("https://public.api.bsky.app").unwrap().to_owned();
/// // let resp = http.xrpc(base).send(&request).await?;
/// # Ok(())
/// # }
/// ```
pub trait XrpcExt: HttpClient {
    /// Start building an XRPC call for the given base URI.
    fn xrpc<'a>(&'a self, base: Uri<&'a str>) -> XrpcCall<'a, Self>
    where
        Self: Sized,
    {
        XrpcCall {
            client: self,
            base,
            opts: CallOptions::default(),
        }
    }
}

impl<T: HttpClient> XrpcExt for T {}

/// Nicer alias for Xrpc response type
pub type XrpcResponse<R> = Response<<R as XrpcRequest>::Response>;

/// Stateful XRPC call trait
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait XrpcClient: HttpClient {
    /// Get the base URI for the client.
    fn base_uri(&self) -> impl Future<Output = Uri<String>>;

    /// Set the base URI for the client.
    ///
    /// The implementation should strip any trailing slash from the URI path before storing.
    fn set_base_uri(&self, uri: Uri<String>) -> impl Future<Output = ()> {
        let _ = uri;
        async {}
    }

    /// Get the call options for the client.
    fn opts(&self) -> impl Future<Output = CallOptions> {
        async { CallOptions::default() }
    }

    /// Set the call options for the client.
    fn set_opts(&self, opts: CallOptions) -> impl Future<Output = ()> {
        let _ = opts;
        async {}
    }

    /// Send an XRPC request and parse the response
    #[cfg(not(target_arch = "wasm32"))]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync;

    /// Send an XRPC request and parse the response
    #[cfg(target_arch = "wasm32")]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync;

    /// Send an XRPC request and parse the response
    #[cfg(not(target_arch = "wasm32"))]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync;

    /// Send an XRPC request with custom options and parse the response
    #[cfg(target_arch = "wasm32")]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync;
}

/// Stateful XRPC streaming client trait
#[cfg(feature = "streaming")]
pub trait XrpcStreamingClient: XrpcClient + HttpClientExt {
    /// Send an XRPC request and stream the response
    #[cfg(not(target_arch = "wasm32"))]
    fn download<R>(
        &self,
        request: R,
    ) -> impl Future<Output = Result<StreamingResponse, StreamError>> + Send
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync;

    /// Send an XRPC request and stream the response
    #[cfg(target_arch = "wasm32")]
    fn download<R>(
        &self,
        request: R,
    ) -> impl Future<Output = Result<StreamingResponse, StreamError>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync;

    /// Stream an XRPC procedure call and its response
    #[cfg(not(target_arch = "wasm32"))]
    fn stream<S, B>(
        &self,
        stream: XrpcProcedureSend<S::Frame<B>>,
    ) -> impl Future<
        Output = Result<
            XrpcResponseStream<<<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>>,
            StreamError,
        >,
    >
    where
        B: BosStr + 'static,
        S: XrpcProcedureStream + 'static,
        <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>: XrpcStreamResp,
        Self: Sync;

    /// Stream an XRPC procedure call and its response
    #[cfg(target_arch = "wasm32")]
    fn stream<S, B>(
        &self,
        stream: XrpcProcedureSend<S::Frame<B>>,
    ) -> impl Future<
        Output = Result<
            XrpcResponseStream<<<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>>,
            StreamError,
        >,
    >
    where
        B: BosStr + 'static,
        S: XrpcProcedureStream + 'static,
        <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>: XrpcStreamResp;
}

/// Stateless XRPC call builder.
///
/// Example (per-request overrides)
/// ```no_run
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use jacquard_common::xrpc::XrpcExt;
/// use jacquard_common::AuthorizationToken;
/// use jacquard_common::deps::fluent_uri::Uri;
///
/// let http = reqwest::Client::new();
/// let base = Uri::parse("https://public.api.bsky.app").unwrap();
/// let call = http
///     .xrpc(base)
///     .auth(AuthorizationToken::Bearer("ACCESS_JWT".into()))
///     .accept_labelers(vec!["did:plc:labelerid".into()])
///     .header(http::header::USER_AGENT, http::HeaderValue::from_static("jacquard-example"));
/// // let resp = call.send(&request).await?;
/// # Ok(())
/// # }
/// ```
pub struct XrpcCall<'a, C: HttpClient> {
    pub(crate) client: &'a C,
    pub(crate) base: Uri<&'a str>,
    pub(crate) opts: CallOptions,
}

impl<'a, C: HttpClient> XrpcCall<'a, C> {
    /// Apply Authorization to this call.
    pub fn auth(mut self, token: AuthorizationToken) -> Self {
        self.opts.auth = Some(token);
        self
    }
    /// Set `atproto-proxy` header for this call.
    pub fn proxy(mut self, proxy: DefaultStr) -> Self {
        self.opts.atproto_proxy = Some(proxy);
        self
    }
    /// Set `atproto-accept-labelers` header(s) for this call.
    pub fn accept_labelers(mut self, labelers: Vec<DefaultStr>) -> Self {
        self.opts.atproto_accept_labelers = Some(labelers);
        self
    }
    /// Add an extra header.
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.opts.extra_headers.push((name, value));
        self
    }
    /// Replace the builder's options entirely.
    pub fn with_options(mut self, opts: CallOptions) -> Self {
        self.opts = opts;
        self
    }

    /// Send the given typed XRPC request and return a response wrapper.
    ///
    /// Note on 401 handling:
    /// - When the server returns 401 with a `WWW-Authenticate` header, this surfaces as
    ///   `ClientError::Auth(AuthError::Other(header))` so higher layers (e.g., OAuth/DPoP) can
    ///   inspect the header for `error="invalid_token"` or `error="use_dpop_nonce"` and react
    ///   (refresh/retry). If the header is absent, the 401 body flows through to `Response` and
    ///   can be parsed/mapped to `AuthError` as appropriate.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug", skip(self, request), fields(nsid = R::NSID)))]
    pub async fn send<R>(self, request: &R) -> XrpcResult<Response<<R as XrpcRequest>::Response>>
    where
        R: XrpcRequest + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let http_request = build_http_request(&self.base, request, &self.opts)?;

        let http_response = self
            .client
            .send_http(http_request)
            .await
            .map_err(|e| crate::error::ClientError::transport(e).for_nsid(R::NSID))?;

        process_response(http_response)
    }
}

/// Process the HTTP response from the server into a proper xrpc response statelessly.
///
/// Exposed to make things more easily pluggable
#[inline]
pub fn process_response<Resp>(http_response: http::Response<Vec<u8>>) -> XrpcResult<Response<Resp>>
where
    Resp: XrpcResp,
{
    let status = http_response.status();

    // If the server returned 401 with a WWW-Authenticate header, expose it so higher layers
    // (e.g., DPoP handling) can detect `error="invalid_token"` and trigger refresh.
    #[allow(deprecated)]
    if status.as_u16() == 401 {
        if let Some(hv) = http_response.headers().get(http::header::WWW_AUTHENTICATE) {
            return Err(
                crate::error::ClientError::auth(crate::error::AuthError::Other(hv.clone()))
                    .for_nsid(Resp::NSID),
            );
        }
    }
    let buffer = Bytes::from(http_response.into_body());

    if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
        return Err(crate::error::ClientError::from(crate::error::HttpError {
            status,
            body: Some(buffer),
        })
        .for_nsid(Resp::NSID));
    }

    Ok(Response::new(buffer, status))
}

/// HTTP headers commonly used in XRPC requests
pub enum Header {
    /// Content-Type header
    ContentType,
    /// Authorization header
    Authorization,
    /// `atproto-proxy` header - specifies which service (app server or other atproto service) the user's PDS should forward requests to as appropriate.
    ///
    /// See: <https://atproto.com/specs/xrpc#service-proxying>
    AtprotoProxy,
    /// `atproto-accept-labelers` header used by clients to request labels from specific labelers to be included and applied in the response. See [label](https://atproto.com/specs/label) specification for details.
    AtprotoAcceptLabelers,
}

impl From<Header> for HeaderName {
    fn from(value: Header) -> Self {
        match value {
            Header::ContentType => CONTENT_TYPE,
            Header::Authorization => AUTHORIZATION,
            Header::AtprotoProxy => HeaderName::from_static("atproto-proxy"),
            Header::AtprotoAcceptLabelers => HeaderName::from_static("atproto-accept-labelers"),
        }
    }
}

/// Construct an XRPC endpoint URI from a base URI, NSID, and optional query string.
///
/// This helper:
/// 1. Extracts scheme and authority from the base URI
/// 2. Gets the base path (already guaranteed no trailing slash from `set_base_uri`)
/// 3. Builds new path: `{base_path}/xrpc/{nsid}`
/// 4. Optionally sets query from serialized parameters
/// 5. Returns the constructed URI
fn xrpc_endpoint_uri(base: &Uri<&str>, nsid: &str, query: Option<&str>) -> XrpcResult<Uri<String>> {
    use crate::error::ClientError;

    let base_path = base.path().as_str().trim_end_matches('/');

    // Calculate approximate capacity: scheme + "://" + authority + base_path + "/xrpc/" + nsid + optional query
    let capacity = base.scheme().as_str().len()
        + 3 // "://"
        + base.authority().map(|a| a.as_str().len()).unwrap_or(0)
        + base_path.len()
        + 6 // "/xrpc/"
        + nsid.len()
        + query.map(|q| q.len() + 1).unwrap_or(0); // query + "?"

    // Build new path string: {base_path}/xrpc/{nsid}
    let mut uri_str = String::with_capacity(capacity);
    uri_str.push_str(base.scheme().as_str());
    uri_str.push_str("://");

    if let Some(authority) = base.authority() {
        uri_str.push_str(authority.as_str());
    }

    uri_str.push_str(base_path);
    uri_str.push_str("/xrpc/");
    uri_str.push_str(nsid);

    if let Some(q) = query {
        uri_str.push('?');
        uri_str.push_str(q);
    }

    Uri::parse(uri_str)
        .map_err(|_| ClientError::invalid_request("Failed to construct XRPC endpoint URI"))
}

/// Build an HTTP request for an XRPC call given base URI and options
pub fn build_http_request<'s, R>(
    base: &Uri<&str>,
    req: &R,
    opts: &CallOptions,
) -> XrpcResult<Request<Vec<u8>>>
where
    R: XrpcRequest + Serialize,
{
    use crate::error::ClientError;

    // Determine query string for Query methods
    let query_string = if let XrpcMethod::Query = <R as XrpcRequest>::METHOD {
        let qs = serde_html_form::to_string(&req).map_err(|e| {
            ClientError::invalid_request(format!("Failed to serialize query: {}", e))
        })?;
        if !qs.is_empty() { Some(qs) } else { None }
    } else {
        None
    };

    // Construct the XRPC endpoint URI using the helper
    let uri = xrpc_endpoint_uri(base, <R as XrpcRequest>::NSID, query_string.as_deref())?;

    let method = match <R as XrpcRequest>::METHOD {
        XrpcMethod::Query => http::Method::GET,
        XrpcMethod::Procedure(_) => http::Method::POST,
    };

    let mut builder = Request::builder().method(method).uri(uri.as_str());

    let has_content_type = opts
        .extra_headers
        .iter()
        .any(|(name, _)| name == CONTENT_TYPE);

    if let XrpcMethod::Procedure(encoding) = <R as XrpcRequest>::METHOD {
        // Only set default Content-Type if not provided in extra_headers
        if !has_content_type {
            builder = builder.header(Header::ContentType, encoding);
        }
    }
    let output_encoding = <R::Response as XrpcResp>::ENCODING;
    builder = builder.header(http::header::ACCEPT, output_encoding);

    if let Some(token) = &opts.auth {
        let hv = match token {
            AuthorizationToken::Bearer(t) | AuthorizationToken::Delegation(t) => {
                HeaderValue::from_str(&format!("Bearer {}", t.as_str()))
            }
            AuthorizationToken::Dpop(t) => HeaderValue::from_str(&format!("DPoP {}", t.as_str())),
        }
        .map_err(|e| ClientError::invalid_request(format!("Invalid authorization token: {}", e)))?;
        builder = builder.header(Header::Authorization, hv);
    }

    if let Some(proxy) = &opts.atproto_proxy {
        builder = builder.header(Header::AtprotoProxy, proxy.as_str());
    }
    if let Some(labelers) = &opts.atproto_accept_labelers {
        if !labelers.is_empty() {
            let joined = labelers
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join(", ");
            builder = builder.header(Header::AtprotoAcceptLabelers, joined);
        }
    }
    for (name, value) in &opts.extra_headers {
        builder = builder.header(name, value);
    }

    let body = if let XrpcMethod::Procedure(_) = R::METHOD {
        let mut buf = Vec::with_capacity(300);
        req.encode_body(&mut buf)
            .map_err(|e| ClientError::invalid_request(format!("Failed to encode body: {}", e)))?;
        buf
    } else {
        vec![]
    };

    builder
        .body(body)
        .map_err(|e| ClientError::invalid_request(format!("Failed to build request: {}", e)))
}

/// XRPC response wrapper that owns the response buffer
///
/// Allows borrowing from the buffer when parsing to avoid unnecessary allocations.
/// Generic over the response marker type (e.g., `GetAuthorFeedResponse`), not the request.
pub struct Response<Resp>
where
    Resp: XrpcResp, // HRTB: Resp works with any lifetime
{
    _marker: PhantomData<fn() -> Resp>,
    buffer: Bytes,
    status: StatusCode,
}

impl<R> Response<R>
where
    R: XrpcResp,
{
    /// Create a new response from a buffer and status code
    pub fn new(buffer: Bytes, status: StatusCode) -> Self {
        Self {
            buffer,
            status,
            _marker: PhantomData,
        }
    }

    /// Get the HTTP status code
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Get the raw buffer
    pub fn buffer(&self) -> &Bytes {
        &self.buffer
    }

    /// Parse the response with caller-chosen backing string type.
    ///
    /// Use turbofish to select: `response.parse::<CowStr<'_>>()` for zero-copy,
    /// `response.parse::<SmolStr>()` for owned.
    pub fn parse<'s, S>(&'s self) -> Result<R::Output<S>, XrpcError<R::Err>>
    where
        S: BosStr + Deserialize<'s>,
        R::Output<S>: Deserialize<'s>,
    {
        if self.status.is_success() {
            R::decode_output::<S>(&self.buffer).map_err(XrpcError::Decode)
        } else {
            Err(self.parse_error())
        }
    }

    /// Parse this as validated, loosely typed atproto data.
    ///
    /// Returns `Data<CowStr>` borrowing from the buffer where possible.
    /// If the response is an error, it will still parse as the matching error type for the request.
    pub fn parse_data(&self) -> Result<Data<CowStr<'_>>, XrpcError<R::Err>> {
        if self.status.is_success() {
            match serde_json::from_slice::<Data<CowStr<'_>>>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(_) => {
                    if let Ok(ipld) = serde_ipld_dagcbor::from_slice::<Ipld>(&self.buffer) {
                        if let Ok(data) = RawData::from_cbor(&ipld) {
                            // CBOR path always allocates, convert RawData → Data via SmolStr.
                            Ok(data
                                .into_static()
                                .try_into()
                                .unwrap_or(Data::Bytes(self.buffer.clone())))
                        } else {
                            Ok(Data::Bytes(self.buffer.clone()))
                        }
                    } else {
                        Ok(Data::Bytes(self.buffer.clone()))
                    }
                }
            }
        } else {
            Err(self.parse_error())
        }
    }

    /// Parse this as raw atproto data with minimal validation.
    ///
    /// If the response is an error, it will still parse as the matching error type for the request.
    pub fn parse_raw(&self) -> Result<RawData<'_>, XrpcError<R::Err>> {
        if self.status.is_success() {
            match serde_json::from_slice::<RawData<'_>>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(_) => {
                    if let Ok(ipld) = serde_ipld_dagcbor::from_slice::<Ipld>(&self.buffer) {
                        if let Ok(data) = RawData::from_cbor(&ipld) {
                            Ok(data.into_static())
                        } else {
                            Ok(RawData::Bytes(self.buffer.clone()))
                        }
                    } else {
                        Ok(RawData::Bytes(self.buffer.clone()))
                    }
                }
            }
        } else {
            Err(self.parse_error())
        }
    }

    /// Parse error response body. Errors are always owned (`DeserializeOwned`).
    fn parse_error(&self) -> XrpcError<R::Err> {
        // 400: try typed XRPC error, fallback to generic error.
        if self.status.as_u16() == 400 {
            match serde_json::from_slice::<R::Err>(&self.buffer) {
                Ok(error) => {
                    use alloc::string::ToString;
                    if error.to_string().contains("InvalidToken") {
                        XrpcError::Auth(AuthError::InvalidToken)
                    } else if error.to_string().contains("ExpiredToken") {
                        XrpcError::Auth(AuthError::TokenExpired)
                    } else {
                        XrpcError::Xrpc(error)
                    }
                }
                Err(_) => self.parse_generic_error(),
            }
        // 401: always auth error.
        } else {
            self.parse_generic_error()
        }
    }

    /// Fallback: parse as generic XRPC error (InvalidRequest, ExpiredToken, etc.).
    fn parse_generic_error(&self) -> XrpcError<R::Err> {
        match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
            Ok(mut generic) => {
                generic.nsid = R::NSID;
                generic.method = "";
                generic.http_status = self.status;
                match generic.error.as_str() {
                    "ExpiredToken" => XrpcError::Auth(AuthError::TokenExpired),
                    "InvalidToken" => XrpcError::Auth(AuthError::InvalidToken),
                    _ => XrpcError::Generic(generic),
                }
            }
            Err(e) => XrpcError::Decode(DecodeError::Json(e)),
        }
    }

    /// Reinterpret this response as a different response type.
    ///
    /// This transmutes the response by keeping the same buffer and status code,
    /// but changing the type-level marker. Useful for converting generic XRPC responses
    /// into collection-specific typed responses.
    ///
    /// # Invariants
    ///
    /// This is safe in the sense that no memory unsafety occurs, but logical correctness
    /// depends on ensuring the buffer actually contains data that can deserialize to `NEW`.
    /// Incorrect conversion will cause deserialization errors at runtime.
    pub fn transmute<NEW: XrpcResp>(self) -> Response<NEW> {
        Response {
            buffer: self.buffer,
            status: self.status,
            _marker: PhantomData,
        }
    }
}

/// Output type alias for a given response marker and backing string type.
pub type RespOutput<S, Resp> = <Resp as XrpcResp>::Output<S>;
/// Error type alias for a given response marker.
pub type RespErr<Resp> = <Resp as XrpcResp>::Err;

impl<R> Response<R>
where
    R: XrpcResp,
{
    /// Parse the response into an owned `SmolStr`-backed output.
    pub fn into_output(self) -> Result<R::Output<SmolStr>, XrpcError<R::Err>>
    where
        R::Output<SmolStr>: DeserializeOwned,
    {
        if self.status.is_success() {
            R::decode_output::<SmolStr>(&self.buffer).map_err(XrpcError::Decode)
        } else {
            Err(self.parse_error())
        }
    }
}

/// Generic XRPC error format for untyped errors like InvalidRequest
///
/// Used when the error doesn't match the endpoint's specific error enum
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenericXrpcError {
    /// Error code (e.g., "InvalidRequest")
    pub error: SmolStr,
    /// Optional error message with details
    pub message: Option<SmolStr>,
    /// XRPC method NSID that produced this error (context only; not serialized)
    #[serde(skip)]
    pub nsid: &'static str,
    /// HTTP method used (GET/POST) (context only; not serialized)
    #[serde(skip)]
    pub method: &'static str,
    /// HTTP status code (context only; not serialized)
    #[serde(skip)]
    pub http_status: StatusCode,
}

impl core::fmt::Display for GenericXrpcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(msg) = &self.message {
            write!(
                f,
                "{}: {} (nsid={}, method={}, status={})",
                self.error, msg, self.nsid, self.method, self.http_status
            )
        } else {
            write!(
                f,
                "{} (nsid={}, method={}, status={})",
                self.error, self.nsid, self.method, self.http_status
            )
        }
    }
}

impl core::error::Error for GenericXrpcError {}

/// XRPC-specific errors returned from endpoints
///
/// Represents errors returned in the response body.
/// Type parameter `E` is the endpoint's specific error enum type, which is always
/// `DeserializeOwned` (SmolStr-backed, no lifetime).
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(miette::Diagnostic))]
#[non_exhaustive]
pub enum XrpcError<E: core::error::Error> {
    /// Typed XRPC error from the endpoint's specific error enum
    #[error("XRPC error: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard_common::xrpc::typed)))]
    Xrpc(E),

    /// Authentication error (ExpiredToken, InvalidToken, etc.)
    #[error("Authentication error: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard_common::xrpc::auth)))]
    Auth(#[from] AuthError),

    /// Generic XRPC error not in the endpoint's error enum (e.g., InvalidRequest)
    #[error("XRPC error: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard_common::xrpc::generic)))]
    Generic(GenericXrpcError),

    /// Failed to decode the response body
    #[error("Failed to decode response: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard_common::xrpc::decode)))]
    Decode(#[from] DecodeError),
}

impl<E> Serialize for XrpcError<E>
where
    E: core::error::Error + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        match self {
            // Typed errors already serialize to correct atproto format
            XrpcError::Xrpc(e) => e.serialize(serializer),
            // Generic errors already have correct format
            XrpcError::Generic(g) => g.serialize(serializer),
            // Auth and Decode need manual mapping to {"error": "...", "message": ...}
            XrpcError::Auth(auth) => {
                let mut state = serializer.serialize_struct("XrpcError", 2)?;
                let (error, message) = match auth {
                    AuthError::TokenExpired => ("ExpiredToken", Some("Access token has expired")),
                    AuthError::InvalidToken => {
                        ("InvalidToken", Some("Access token is invalid or malformed"))
                    }
                    AuthError::RefreshFailed => {
                        ("RefreshFailed", Some("Token refresh request failed"))
                    }
                    AuthError::NotAuthenticated => (
                        "AuthenticationRequired",
                        Some("Request requires authentication but none was provided"),
                    ),
                    AuthError::DpopProofFailed => {
                        ("DpopProofFailed", Some("DPoP proof construction failed"))
                    }
                    AuthError::DpopNonceFailed => {
                        ("DpopNonceFailed", Some("DPoP nonce negotiation failed"))
                    }
                    AuthError::Other(hv) => {
                        let msg = hv.to_str().unwrap_or("[non-utf8 header]");
                        ("AuthenticationError", Some(msg))
                    }
                };
                state.serialize_field("error", error)?;
                if let Some(msg) = message {
                    state.serialize_field("message", msg)?;
                }
                state.end()
            }
            XrpcError::Decode(decode_err) => {
                let mut state = serializer.serialize_struct("XrpcError", 2)?;
                state.serialize_field("error", "ResponseDecodeError")?;
                // Convert DecodeError to string for message field
                let msg = format!("{:?}", decode_err);
                state.serialize_field("message", &msg)?;
                state.end()
            }
        }
    }
}

#[cfg(feature = "streaming")]
impl<'a, C: HttpClient + HttpClientExt> XrpcCall<'a, C> {
    /// Send an XRPC call and stream the binary response.
    ///
    /// Useful for downloading blobs and entire repository archives
    pub async fn download<R>(self, request: &R) -> Result<StreamingResponse, StreamError>
    where
        R: XrpcRequest + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let http_request =
            build_http_request(&self.base, request, &self.opts).map_err(StreamError::transport)?;

        let http_response = self
            .client
            .send_http_streaming(http_request)
            .await
            .map_err(StreamError::transport)?;
        let (parts, body) = http_response.into_parts();

        Ok(StreamingResponse::new(parts, body))
    }

    /// Stream an XRPC procedure call and its response
    ///
    /// Useful for streaming upload of large payloads, or for "pipe-through" operations
    /// where you are processing a large payload.
    pub async fn stream<S, B>(
        self,
        stream: XrpcProcedureSend<S::Frame<B>>,
    ) -> Result<XrpcResponseStream<<S::Response as XrpcStreamResp>::Frame<B>>, StreamError>
    where
        S: XrpcProcedureStream + 'static,
        B: BosStr + 'static,
        <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>: XrpcStreamResp,
    {
        use alloc::boxed::Box;
        use futures::TryStreamExt;

        let uri = xrpc_endpoint_uri(&self.base, <S::Request as XrpcRequest>::NSID, None).map_err(
            |e| StreamError::protocol(format!("Failed to construct endpoint URI: {}", e)),
        )?;

        let mut builder = http::Request::post(uri.as_str());

        if let Some(token) = &self.opts.auth {
            let hv = match token {
                AuthorizationToken::Bearer(t) => {
                    HeaderValue::from_str(&format!("Bearer {}", t.as_str()))
                }
                AuthorizationToken::Dpop(t) => {
                    HeaderValue::from_str(&format!("DPoP {}", t.as_str()))
                }
                AuthorizationToken::Delegation(t) => {
                    HeaderValue::from_str(&format!("DPoP {}", t.as_str()))
                }
            }
            .map_err(|e| StreamError::protocol(format!("Invalid authorization token: {}", e)))?;
            builder = builder.header(Header::Authorization, hv);
        }

        if let Some(proxy) = &self.opts.atproto_proxy {
            builder = builder.header(Header::AtprotoProxy, proxy.as_str());
        }
        if let Some(labelers) = &self.opts.atproto_accept_labelers {
            if !labelers.is_empty() {
                let joined = labelers
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>()
                    .join(", ");
                builder = builder.header(Header::AtprotoAcceptLabelers, joined);
            }
        }

        for (name, value) in &self.opts.extra_headers {
            builder = builder.header(name, value);
        }

        let (parts, _) = builder
            .body(())
            .map_err(|e| StreamError::protocol(e.to_string()))?
            .into_parts();

        let body_stream = Box::pin(stream.0.map_ok(|f| f.buffer));

        let resp = self
            .client
            .send_http_bidirectional(parts, body_stream)
            .await
            .map_err(StreamError::transport)?;

        let (parts, body) = resp.into_parts();

        Ok(XrpcResponseStream::<
            <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>,
        >::from_typed_parts::<B>(parts, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[allow(dead_code)]
    struct DummyReq;

    #[derive(Deserialize, Serialize, Debug, thiserror::Error)]
    #[error("{0}")]
    struct DummyErr(SmolStr);

    struct DummyResp;

    impl XrpcResp for DummyResp {
        const NSID: &'static str = "test.dummy";
        const ENCODING: &'static str = "application/json";
        type Output<S: BosStr> = ();
        type Err = DummyErr;
    }

    impl XrpcRequest for DummyReq {
        const NSID: &'static str = "test.dummy";
        const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
        type Response = DummyResp;
    }

    #[test]
    fn generic_error_carries_context() {
        let body = serde_json::json!({"error":"InvalidRequest","message":"missing"});
        let buf = Bytes::from(serde_json::to_vec(&body).unwrap());
        let resp: Response<DummyResp> = Response::new(buf, StatusCode::BAD_REQUEST);
        match resp.parse::<SmolStr>().unwrap_err() {
            XrpcError::Generic(g) => {
                assert_eq!(g.error.as_str(), "InvalidRequest");
                assert_eq!(g.message.as_deref(), Some("missing"));
                assert_eq!(g.nsid, DummyResp::NSID);
                assert_eq!(g.method, ""); // method info only on request
                assert_eq!(g.http_status, StatusCode::BAD_REQUEST);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn auth_error_mapping() {
        for (code, expect) in [
            ("ExpiredToken", AuthError::TokenExpired),
            ("InvalidToken", AuthError::InvalidToken),
        ] {
            let body = serde_json::json!({"error": code});
            let buf = Bytes::from(serde_json::to_vec(&body).unwrap());
            let resp: Response<DummyResp> = Response::new(buf, StatusCode::UNAUTHORIZED);
            match resp.parse::<SmolStr>().unwrap_err() {
                XrpcError::Auth(e) => match (e, expect) {
                    (AuthError::TokenExpired, AuthError::TokenExpired) => {}
                    (AuthError::InvalidToken, AuthError::InvalidToken) => {}
                    other => panic!("mismatch: {other:?}"),
                },
                other => panic!("unexpected: {other:?}"),
            }
        }
    }

    #[test]
    fn xrpc_uri_construction_basic() {
        use crate::alloc::string::ToString;
        #[derive(Serialize, Deserialize)]
        struct Req;
        #[derive(Deserialize, Serialize, Debug, thiserror::Error)]
        #[error("test error")]
        struct Err;
        struct Resp;
        impl XrpcResp for Resp {
            const NSID: &'static str = "com.example.test";
            const ENCODING: &'static str = "application/json";
            type Output<S: BosStr> = ();
            type Err = Err;
        }
        impl XrpcRequest for Req {
            const NSID: &'static str = "com.example.test";
            const METHOD: XrpcMethod = XrpcMethod::Query;
            type Response = Resp;
        }

        let opts = CallOptions::default();

        // AC1.1: Base URI without trailing slash + NSID produces correct `/xrpc/{nsid}` path
        let base1 = Uri::parse("https://pds.example.com").expect("URI should be valid");
        let req1 = build_http_request(&base1, &Req, &opts).unwrap();
        let uri1 = req1.uri().to_string();
        assert!(
            uri1.contains("/xrpc/com.example.test"),
            "AC1.1: URI {} should contain '/xrpc/com.example.test'",
            uri1
        );
        assert_eq!(
            uri1, "https://pds.example.com/xrpc/com.example.test",
            "AC1.1: URI should be exact match"
        );

        // AC1.2: Base URI with sub-path preserves it: `/base/xrpc/{nsid}`
        let base2 = Uri::parse("https://pds.example.com/base").expect("URI should be valid");
        let req2 = build_http_request(&base2, &Req, &opts).unwrap();
        let uri2 = req2.uri().to_string();
        assert!(
            uri2.contains("/base/xrpc/com.example.test"),
            "AC1.2: URI {} should contain '/base/xrpc/com.example.test'",
            uri2
        );
        assert_eq!(
            uri2, "https://pds.example.com/base/xrpc/com.example.test",
            "AC1.2: URI should preserve sub-path"
        );

        // AC1.5: Base URI with trailing slash is normalized (slash stripped) before construction
        let base_with_slash = Uri::parse("https://pds.example.com/").expect("URI should be valid");
        let req_slash = build_http_request(&base_with_slash, &Req, &opts).unwrap();
        let uri_slash = req_slash.uri().to_string();
        assert!(
            !uri_slash.contains("//xrpc"),
            "AC1.5: URI {} should not contain '//xrpc'",
            uri_slash
        );
        assert_eq!(
            uri_slash, "https://pds.example.com/xrpc/com.example.test",
            "AC1.5: URI should handle trailing slash"
        );
    }

    #[test]
    fn xrpc_uri_query_parameters() {
        use crate::alloc::string::ToString;
        use serde::Serialize;

        #[derive(Serialize)]
        struct QueryReq {
            #[serde(skip_serializing_if = "Option::is_none")]
            param1: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            param2: Option<String>,
        }

        #[derive(Serialize, Deserialize, Debug, thiserror::Error)]
        #[error("test error")]
        struct Err;
        struct Resp;
        impl XrpcResp for Resp {
            const NSID: &'static str = "com.example.test";
            const ENCODING: &'static str = "application/json";
            type Output<S: BosStr> = ();
            type Err = Err;
        }
        impl XrpcRequest for QueryReq {
            const NSID: &'static str = "com.example.test";
            const METHOD: XrpcMethod = XrpcMethod::Query;
            type Response = Resp;
        }

        let opts = CallOptions::default();
        let base = Uri::parse("https://pds.example.com").expect("URI should be valid");

        // AC1.3: Query parameters from serde serialisation are set correctly
        let req_with_params = QueryReq {
            param1: Some("value1".to_string()),
            param2: Some("value2".to_string()),
        };
        let http_req = build_http_request(&base, &req_with_params, &opts).unwrap();
        let uri_str = http_req.uri().to_string();
        assert!(
            uri_str.contains("?"),
            "AC1.3: URI should contain query string"
        );
        assert!(
            uri_str.contains("param1=value1"),
            "AC1.3: URI should contain param1"
        );
        assert!(
            uri_str.contains("param2=value2"),
            "AC1.3: URI should contain param2"
        );

        // AC1.4: Empty/default query parameters result in no `?` in the constructed URI
        let req_empty_params = QueryReq {
            param1: None,
            param2: None,
        };
        let http_req_empty = build_http_request(&base, &req_empty_params, &opts).unwrap();
        let uri_str_empty = http_req_empty.uri().to_string();
        assert!(
            !uri_str_empty.contains("?"),
            "AC1.4: URI {} should not contain '?' with empty params",
            uri_str_empty
        );
        assert_eq!(
            uri_str_empty, "https://pds.example.com/xrpc/com.example.test",
            "AC1.4: URI should have no query string"
        );
    }

    #[test]
    fn xrpc_uri_special_characters_in_query() {
        use crate::alloc::string::ToString;
        use serde::Serialize;

        #[derive(Serialize)]
        struct QueryReq {
            #[serde(skip_serializing_if = "Option::is_none")]
            search: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            filter: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            unicode_param: Option<String>,
        }

        #[derive(Serialize, Deserialize, Debug, thiserror::Error)]
        #[error("test error")]
        struct Err;
        struct Resp;
        impl XrpcResp for Resp {
            const NSID: &'static str = "com.example.test";
            const ENCODING: &'static str = "application/json";
            type Output<S: BosStr> = ();
            type Err = Err;
        }
        impl XrpcRequest for QueryReq {
            const NSID: &'static str = "com.example.test";
            const METHOD: XrpcMethod = XrpcMethod::Query;
            type Response = Resp;
        }

        let opts = CallOptions::default();
        let base = Uri::parse("https://pds.example.com").expect("URI should be valid");

        // AC1.3: Test with spaces (serde_html_form uses + for spaces per application/x-www-form-urlencoded)
        let req_spaces = QueryReq {
            search: Some("hello world".to_string()),
            filter: None,
            unicode_param: None,
        };
        let http_req_spaces = build_http_request(&base, &req_spaces, &opts).unwrap();
        let uri_spaces = http_req_spaces.uri().to_string();
        assert!(
            uri_spaces.contains("search=hello"),
            "AC1.3: URI should contain search param"
        );
        // serde_html_form encodes spaces as +
        assert!(
            uri_spaces.contains("hello+world") || uri_spaces.contains("hello%20world"),
            "AC1.3: URI {} should encode space in 'hello world'",
            uri_spaces
        );

        // AC1.3: Test with special characters: &, =, +
        let req_special = QueryReq {
            search: Some("a=b&c+d".to_string()),
            filter: None,
            unicode_param: None,
        };
        let http_req_special = build_http_request(&base, &req_special, &opts).unwrap();
        let uri_special = http_req_special.uri().to_string();
        assert!(
            uri_special.contains("?"),
            "AC1.3: URI should contain query string for special chars"
        );
        // Verify the URI can be parsed successfully (fluent-uri handles encoded values)
        let parsed = Uri::parse(uri_special.clone());
        assert!(
            parsed.is_ok(),
            "AC1.3: URI {} should be parseable by fluent-uri",
            uri_special
        );

        // AC1.3: Test with unicode characters
        let req_unicode = QueryReq {
            search: None,
            filter: None,
            unicode_param: Some("你好世界".to_string()),
        };
        let http_req_unicode = build_http_request(&base, &req_unicode, &opts).unwrap();
        let uri_unicode = http_req_unicode.uri().to_string();
        assert!(
            uri_unicode.contains("?"),
            "AC1.3: URI should contain query string for unicode"
        );
        // Verify the URI can be parsed successfully
        let parsed_unicode = Uri::parse(uri_unicode.clone());
        assert!(
            parsed_unicode.is_ok(),
            "AC1.3: URI {} should be parseable for unicode params",
            uri_unicode
        );
    }

    #[test]
    fn no_double_slash_in_path() {
        use crate::alloc::string::ToString;
        #[derive(Serialize, Deserialize)]
        struct Req;
        #[derive(Deserialize, Serialize, Debug, thiserror::Error)]
        #[error("test error")]
        struct Err;
        struct Resp;
        impl XrpcResp for Resp {
            const NSID: &'static str = "com.example.test";
            const ENCODING: &'static str = "application/json";
            type Output<S: BosStr> = ();
            type Err = Err;
        }
        impl XrpcRequest for Req {
            const NSID: &'static str = "com.example.test";
            const METHOD: XrpcMethod = XrpcMethod::Query;
            type Response = Resp;
        }

        let opts = CallOptions::default();

        // Ensure no double slashes in path
        let base1 = Uri::parse("https://pds").expect("URI should be valid");
        let req1 = build_http_request(&base1, &Req, &opts).unwrap();
        let uri1 = req1.uri().to_string();
        assert!(
            !uri1.contains("//xrpc"),
            "URI {} should not contain '//xrpc'",
            uri1
        );

        let base2 = Uri::parse("https://pds/base").expect("URI should be valid");
        let req2 = build_http_request(&base2, &Req, &opts).unwrap();
        let uri2 = req2.uri().to_string();
        assert!(
            !uri2.contains("//xrpc"),
            "URI {} should not contain '//xrpc'",
            uri2
        );
    }
}
