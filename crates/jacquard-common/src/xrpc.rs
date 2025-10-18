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

use ipld_core::ipld::Ipld;
#[cfg(feature = "streaming")]
pub use streaming::{
    StreamingResponse, XrpcProcedureSend, XrpcProcedureStream, XrpcResponseStream, XrpcStreamResp,
};

#[cfg(feature = "websocket")]
pub mod subscription;

#[cfg(feature = "streaming")]
use crate::StreamError;
use crate::http_client::HttpClient;
#[cfg(feature = "streaming")]
use crate::http_client::HttpClientExt;
use crate::types::value::Data;
use crate::{AuthorizationToken, error::AuthError};
use crate::{CowStr, error::XrpcResult};
use crate::{IntoStatic, error::DecodeError};
use crate::{error::TransportError, types::value::RawData};
use bytes::Bytes;
use http::{
    HeaderName, HeaderValue, Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::{self, Debug};
use std::{error::Error, marker::PhantomData};
#[cfg(feature = "websocket")]
pub use subscription::{
    BasicSubscriptionClient, MessageEncoding, SubscriptionCall, SubscriptionClient,
    SubscriptionEndpoint, SubscriptionExt, SubscriptionOptions, SubscriptionResp,
    SubscriptionStream, TungsteniteSubscriptionClient, XrpcSubscription,
};
use url::Url;

/// Error type for encoding XRPC requests
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
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
pub trait XrpcRequest: Serialize {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;

    /// Response type returned from the XRPC call (marker struct)
    type Response: XrpcResp;

    /// Encode the request body for procedures.
    ///
    /// Default implementation serializes to JSON. Override for non-JSON encodings.
    fn encode_body(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Decode the request body for procedures.
    ///
    /// Default implementation deserializes from JSON. Override for non-JSON encodings.
    fn decode_body<'de>(body: &'de [u8]) -> Result<Box<Self>, DecodeError>
    where
        Self: Deserialize<'de>,
    {
        let body: Self = serde_json::from_slice(body).map_err(|e| DecodeError::Json(e))?;

        Ok(Box::new(body))
    }
}

/// Trait for XRPC Response types
///
/// It mirrors the NSID and carries the encoding types as well as Output (success) and Err types
pub trait XrpcResp {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// Output encoding (MIME type)
    const ENCODING: &'static str;

    /// Response output type
    type Output<'de>: Serialize + Deserialize<'de> + IntoStatic;

    /// Error type for this request
    type Err<'de>: Error + Deserialize<'de> + IntoStatic;

    /// Output body encoding function, similar to the request-side type
    fn encode_output(output: &Self::Output<'_>) -> Result<Vec<u8>, EncodeError> {
        Ok(serde_json::to_vec(output)?)
    }

    /// Decode the response output body.
    ///
    /// Default implementation deserializes from JSON. Override for non-JSON encodings.
    fn decode_output<'de>(body: &'de [u8]) -> Result<Self::Output<'de>, DecodeError>
    where
        Self::Output<'de>: Deserialize<'de>,
    {
        let body = serde_json::from_slice(body).map_err(|e| DecodeError::Json(e))?;

        Ok(body)
    }
}

/// XRPC server endpoint trait
///
/// Defines the fully-qualified path and method, as well as request and response types
/// This exists primarily to work around lifetime issues for crates like Axum
/// by moving the lifetime from the trait itself into an associated type.
///
/// It is implemented by the code generation on a marker struct, like the client-side [XrpcResp] trait.
pub trait XrpcEndpoint {
    /// Fully-qualified path ('/xrpc/\[nsid\]') where this endpoint should live on the server
    const PATH: &'static str;
    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;
    /// XRPC Request data type
    type Request<'de>: XrpcRequest + Deserialize<'de> + IntoStatic;
    /// XRPC Response data type
    type Response: XrpcResp;
}

/// Error type for XRPC endpoints that don't define any errors
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GenericError<'a>(#[serde(borrow)] Data<'a>);

impl<'de> fmt::Display for GenericError<'de> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for GenericError<'_> {}

impl IntoStatic for GenericError<'_> {
    type Output = GenericError<'static>;
    fn into_static(self) -> Self::Output {
        GenericError(self.0.into_static())
    }
}

/// Per-request options for XRPC calls.
#[derive(Debug, Default, Clone)]
pub struct CallOptions<'a> {
    /// Optional Authorization to apply (`Bearer` or `DPoP`).
    pub auth: Option<AuthorizationToken<'a>>,
    /// `atproto-proxy` header value.
    pub atproto_proxy: Option<CowStr<'a>>,
    /// `atproto-accept-labelers` header values.
    pub atproto_accept_labelers: Option<Vec<CowStr<'a>>>,
    /// Extra headers to attach to this request.
    pub extra_headers: Vec<(HeaderName, HeaderValue)>,
}

impl IntoStatic for CallOptions<'_> {
    type Output = CallOptions<'static>;

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
///
/// let http = reqwest::Client::new();
/// let base = url::Url::parse("https://public.api.bsky.app")?;
/// // let resp = http.xrpc(base).send(&request).await?;
/// # Ok(())
/// # }
/// ```
pub trait XrpcExt: HttpClient {
    /// Start building an XRPC call for the given base URL.
    fn xrpc<'a>(&'a self, base: Url) -> XrpcCall<'a, Self>
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
    fn base_uri(&self) -> impl Future<Output = Url>;

    /// Get the call options for the client.
    fn opts(&self) -> impl Future<Output = CallOptions<'_>> {
        async { CallOptions::default() }
    }

    /// Send an XRPC request and parse the response
    #[cfg(not(target_arch = "wasm32"))]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync;

    /// Send an XRPC request and parse the response
    #[cfg(target_arch = "wasm32")]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync;

    /// Send an XRPC request and parse the response
    #[cfg(not(target_arch = "wasm32"))]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions<'_>,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync;

    /// Send an XRPC request with custom options and parse the response
    #[cfg(target_arch = "wasm32")]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions<'_>,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync,
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
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync;

    /// Send an XRPC request and stream the response
    #[cfg(target_arch = "wasm32")]
    fn download<R>(
        &self,
        request: R,
    ) -> impl Future<Output = Result<StreamingResponse, StreamError>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync;

    /// Stream an XRPC procedure call and its response
    #[cfg(not(target_arch = "wasm32"))]
    fn stream<S>(
        &self,
        stream: XrpcProcedureSend<S::Frame<'static>>,
    ) -> impl Future<
        Output = Result<
            XrpcResponseStream<
                <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<'static>,
            >,
            StreamError,
        >,
    >
    where
        S: XrpcProcedureStream + 'static,
        <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<'static>: XrpcStreamResp,
        Self: Sync;

    /// Stream an XRPC procedure call and its response
    #[cfg(target_arch = "wasm32")]
    fn stream<S>(
        &self,
        stream: XrpcProcedureSend<S::Frame<'static>>,
    ) -> impl Future<
        Output = Result<
            XrpcResponseStream<
                <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<'static>,
            >,
            StreamError,
        >,
    >
    where
        S: XrpcProcedureStream + 'static,
        <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<'static>: XrpcStreamResp;
}

/// Stateless XRPC call builder.
///
/// Example (per-request overrides)
/// ```no_run
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use jacquard_common::xrpc::XrpcExt;
/// use jacquard_common::{AuthorizationToken, CowStr};
///
/// let http = reqwest::Client::new();
/// let base = url::Url::parse("https://public.api.bsky.app")?;
/// let call = http
///     .xrpc(base)
///     .auth(AuthorizationToken::Bearer(CowStr::from("ACCESS_JWT")))
///     .accept_labelers(vec![CowStr::from("did:plc:labelerid")])
///     .header(http::header::USER_AGENT, http::HeaderValue::from_static("jacquard-example"));
/// // let resp = call.send(&request).await?;
/// # Ok(())
/// # }
/// ```
pub struct XrpcCall<'a, C: HttpClient> {
    pub(crate) client: &'a C,
    pub(crate) base: Url,
    pub(crate) opts: CallOptions<'a>,
}

impl<'a, C: HttpClient> XrpcCall<'a, C> {
    /// Apply Authorization to this call.
    pub fn auth(mut self, token: AuthorizationToken<'a>) -> Self {
        self.opts.auth = Some(token);
        self
    }
    /// Set `atproto-proxy` header for this call.
    pub fn proxy(mut self, proxy: CowStr<'a>) -> Self {
        self.opts.atproto_proxy = Some(proxy);
        self
    }
    /// Set `atproto-accept-labelers` header(s) for this call.
    pub fn accept_labelers(mut self, labelers: Vec<CowStr<'a>>) -> Self {
        self.opts.atproto_accept_labelers = Some(labelers);
        self
    }
    /// Add an extra header.
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.opts.extra_headers.push((name, value));
        self
    }
    /// Replace the builder's options entirely.
    pub fn with_options(mut self, opts: CallOptions<'a>) -> Self {
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
        R: XrpcRequest,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let http_request = build_http_request(&self.base, request, &self.opts)
            .map_err(crate::error::TransportError::from)?;

        let http_response = self
            .client
            .send_http(http_request)
            .await
            .map_err(|e| crate::error::TransportError::Other(Box::new(e)))?;

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
    if status.as_u16() == 401 {
        if let Some(hv) = http_response.headers().get(http::header::WWW_AUTHENTICATE) {
            return Err(crate::error::ClientError::Auth(
                crate::error::AuthError::Other(hv.clone()),
            ));
        }
    }
    let buffer = Bytes::from(http_response.into_body());

    if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
        return Err(crate::error::HttpError {
            status,
            body: Some(buffer),
        }
        .into());
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

/// Build an HTTP request for an XRPC call given base URL and options
pub fn build_http_request<'s, R>(
    base: &Url,
    req: &R,
    opts: &CallOptions<'_>,
) -> core::result::Result<Request<Vec<u8>>, crate::error::TransportError>
where
    R: XrpcRequest,
{
    let mut url = base.clone();
    let mut path = url.path().trim_end_matches('/').to_owned();
    path.push_str("/xrpc/");
    path.push_str(<R as XrpcRequest>::NSID);
    url.set_path(&path);

    if let XrpcMethod::Query = <R as XrpcRequest>::METHOD {
        let qs = serde_html_form::to_string(&req)
            .map_err(|e| crate::error::TransportError::InvalidRequest(e.to_string()))?;
        if !qs.is_empty() {
            url.set_query(Some(&qs));
        } else {
            url.set_query(None);
        }
    }

    let method = match <R as XrpcRequest>::METHOD {
        XrpcMethod::Query => http::Method::GET,
        XrpcMethod::Procedure(_) => http::Method::POST,
    };

    let mut builder = Request::builder().method(method).uri(url.as_str());

    if let XrpcMethod::Procedure(encoding) = <R as XrpcRequest>::METHOD {
        builder = builder.header(Header::ContentType, encoding);
    }
    let output_encoding = <R::Response as XrpcResp>::ENCODING;
    builder = builder.header(http::header::ACCEPT, output_encoding);

    if let Some(token) = &opts.auth {
        let hv = match token {
            AuthorizationToken::Bearer(t) => {
                HeaderValue::from_str(&format!("Bearer {}", t.as_ref()))
            }
            AuthorizationToken::Dpop(t) => HeaderValue::from_str(&format!("DPoP {}", t.as_ref())),
        }
        .map_err(|e| {
            TransportError::InvalidRequest(format!("Invalid authorization token: {}", e))
        })?;
        builder = builder.header(Header::Authorization, hv);
    }

    if let Some(proxy) = &opts.atproto_proxy {
        builder = builder.header(Header::AtprotoProxy, proxy.as_ref());
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
        req.encode_body()
            .map_err(|e| TransportError::InvalidRequest(e.to_string()))?
    } else {
        vec![]
    };

    builder
        .body(body)
        .map_err(|e| TransportError::InvalidRequest(e.to_string()))
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

    /// Parse the response, borrowing from the internal buffer
    pub fn parse<'s>(&'s self) -> Result<RespOutput<'s, R>, XrpcError<RespErr<'s, R>>> {
        // 200: parse as output
        if self.status.is_success() {
            match R::decode_output(&self.buffer) {
                Ok(output) => Ok(output),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match serde_json::from_slice::<_>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = ""; // method info only available on request
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                                _ => Err(XrpcError::Generic(generic)),
                            }
                        }
                        Err(e) => Err(XrpcError::Decode(DecodeError::Json(e))),
                    }
                }
            }
        // 401: always auth error
        } else {
            match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(mut generic) => {
                    generic.nsid = R::NSID;
                    generic.method = ""; // method info only available on request
                    generic.http_status = self.status;
                    match generic.error.as_str() {
                        "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                        "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                        _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                    }
                }
                Err(e) => Err(XrpcError::Decode(DecodeError::Json(e))),
            }
        }
    }

    /// Parse this as validated, loosely typed atproto data.
    ///
    /// NOTE: If the response is an error, it will still parse as the matching error type for the request.
    pub fn parse_data<'s>(&'s self) -> Result<Data<'s>, XrpcError<RespErr<'s, R>>> {
        // 200: parse as output
        if self.status.is_success() {
            match serde_json::from_slice::<_>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(_) => {
                    if let Ok(data) = serde_ipld_dagcbor::from_slice::<Ipld>(&self.buffer) {
                        if let Ok(data) = Data::from_cbor(&data) {
                            Ok(data.into_static())
                        } else {
                            Ok(Data::Bytes(self.buffer.clone()))
                        }
                    } else {
                        Ok(Data::Bytes(self.buffer.clone()))
                    }
                }
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match serde_json::from_slice::<_>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = ""; // method info only available on request
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                                _ => Err(XrpcError::Generic(generic)),
                            }
                        }
                        Err(e) => Err(XrpcError::Decode(DecodeError::Json(e))),
                    }
                }
            }
        // 401: always auth error
        } else {
            match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(mut generic) => {
                    generic.nsid = R::NSID;
                    generic.method = ""; // method info only available on request
                    generic.http_status = self.status;
                    match generic.error.as_str() {
                        "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                        "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                        _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                    }
                }
                Err(e) => Err(XrpcError::Decode(DecodeError::Json(e))),
            }
        }
    }

    /// Parse this as raw atproto data with minimal validation.
    ///
    /// NOTE: If the response is an error, it will still parse as the matching error type for the request.
    pub fn parse_raw<'s>(&'s self) -> Result<RawData<'s>, XrpcError<RespErr<'s, R>>> {
        // 200: parse as output
        if self.status.is_success() {
            match serde_json::from_slice::<_>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(_) => {
                    if let Ok(data) = serde_ipld_dagcbor::from_slice::<Ipld>(&self.buffer) {
                        if let Ok(data) = RawData::from_cbor(&data) {
                            Ok(data.into_static())
                        } else {
                            Ok(RawData::Bytes(self.buffer.clone()))
                        }
                    } else {
                        Ok(RawData::Bytes(self.buffer.clone()))
                    }
                }
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match serde_json::from_slice::<_>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = ""; // method info only available on request
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                                _ => Err(XrpcError::Generic(generic)),
                            }
                        }
                        Err(e) => Err(XrpcError::Decode(DecodeError::Json(e))),
                    }
                }
            }
        // 401: always auth error
        } else {
            match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(mut generic) => {
                    generic.nsid = R::NSID;
                    generic.method = ""; // method info only available on request
                    generic.http_status = self.status;
                    match generic.error.as_str() {
                        "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                        "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                        _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                    }
                }
                Err(e) => Err(XrpcError::Decode(DecodeError::Json(e))),
            }
        }
    }

    /// Reinterpret this response as a different response type.
    ///
    /// This transmutes the response by keeping the same buffer and status code,
    /// but changing the type-level marker. Useful for converting generic XRPC responses
    /// into collection-specific typed responses.
    ///
    /// # Safety
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

/// doc
pub type RespOutput<'a, Resp> = <Resp as XrpcResp>::Output<'a>;
/// doc
pub type RespErr<'a, Resp> = <Resp as XrpcResp>::Err<'a>;

impl<R> Response<R>
where
    R: XrpcResp,
{
    /// Parse the response into an owned output
    pub fn into_output(self) -> Result<RespOutput<'static, R>, XrpcError<RespErr<'static, R>>>
    where
        for<'a> RespOutput<'a, R>: IntoStatic<Output = RespOutput<'static, R>>,
        for<'a> RespErr<'a, R>: IntoStatic<Output = RespErr<'static, R>>,
    {
        fn parse_error<'b, R: XrpcResp>(buffer: &'b [u8]) -> Result<R::Err<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match R::decode_output(&self.buffer) {
                Ok(output) => Ok(output.into_static()),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            let error = match parse_error::<R>(&self.buffer) {
                Ok(error) => XrpcError::Xrpc(error),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = ""; // method info only available on request
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_ref() {
                                "ExpiredToken" => XrpcError::Auth(AuthError::TokenExpired),
                                "InvalidToken" => XrpcError::Auth(AuthError::InvalidToken),
                                _ => XrpcError::Generic(generic),
                            }
                        }
                        Err(e) => XrpcError::Decode(DecodeError::Json(e)),
                    }
                }
            };
            Err(error.into_static())
        // 401: always auth error
        } else {
            let error: XrpcError<<R as XrpcResp>::Err<'_>> =
                match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                    Ok(mut generic) => {
                        let status = self.status;
                        generic.nsid = R::NSID;
                        generic.method = ""; // method info only available on request
                        generic.http_status = status;
                        match generic.error.as_ref() {
                            "ExpiredToken" => XrpcError::Auth(AuthError::TokenExpired),
                            "InvalidToken" => XrpcError::Auth(AuthError::InvalidToken),
                            _ => XrpcError::Auth(AuthError::NotAuthenticated),
                        }
                    }
                    Err(e) => XrpcError::Decode(DecodeError::Json(e)),
                };

            Err(error.into_static())
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

impl std::fmt::Display for GenericXrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl IntoStatic for GenericXrpcError {
    type Output = Self;

    fn into_static(self) -> Self::Output {
        self
    }
}

impl std::error::Error for GenericXrpcError {}

/// XRPC-specific errors returned from endpoints
///
/// Represents errors returned in the response body
/// Type parameter `E` is the endpoint's specific error enum type.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum XrpcError<E: std::error::Error + IntoStatic> {
    /// Typed XRPC error from the endpoint's specific error enum
    #[error("XRPC error: {0}")]
    #[diagnostic(code(jacquard_common::xrpc::typed))]
    Xrpc(E),

    /// Authentication error (ExpiredToken, InvalidToken, etc.)
    #[error("Authentication error: {0}")]
    #[diagnostic(code(jacquard_common::xrpc::auth))]
    Auth(#[from] AuthError),

    /// Generic XRPC error not in the endpoint's error enum (e.g., InvalidRequest)
    #[error("XRPC error: {0}")]
    #[diagnostic(code(jacquard_common::xrpc::generic))]
    Generic(GenericXrpcError),

    /// Failed to decode the response body
    #[error("Failed to decode response: {0}")]
    #[diagnostic(code(jacquard_common::xrpc::decode))]
    Decode(#[from] DecodeError),
}

impl<E> IntoStatic for XrpcError<E>
where
    E: std::error::Error + IntoStatic,
    E::Output: std::error::Error + IntoStatic,
    <E as IntoStatic>::Output: std::error::Error + IntoStatic,
{
    type Output = XrpcError<E::Output>;
    fn into_static(self) -> Self::Output {
        match self {
            XrpcError::Xrpc(e) => XrpcError::Xrpc(e.into_static()),
            XrpcError::Auth(e) => XrpcError::Auth(e.into_static()),
            XrpcError::Generic(e) => XrpcError::Generic(e),
            XrpcError::Decode(e) => XrpcError::Decode(e),
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
        R: XrpcRequest,
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
    pub async fn stream<S>(
        self,
        stream: XrpcProcedureSend<S::Frame<'static>>,
    ) -> Result<XrpcResponseStream<<S::Response as XrpcStreamResp>::Frame<'static>>, StreamError>
    where
        S: XrpcProcedureStream + 'static,
        <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<'static>: XrpcStreamResp,
    {
        use futures::TryStreamExt;
        use n0_future::StreamExt;

        let mut url = self.base;
        let mut path = url.path().trim_end_matches('/').to_owned();
        path.push_str("/xrpc/");
        path.push_str(<S::Request as XrpcRequest>::NSID);
        url.set_path(&path);

        let mut builder = http::Request::post(url.to_string());

        if let Some(token) = &self.opts.auth {
            let hv = match token {
                AuthorizationToken::Bearer(t) => {
                    HeaderValue::from_str(&format!("Bearer {}", t.as_ref()))
                }
                AuthorizationToken::Dpop(t) => {
                    HeaderValue::from_str(&format!("DPoP {}", t.as_ref()))
                }
            }
            .map_err(|e| StreamError::protocol(format!("Invalid authorization token: {}", e)))?;
            builder = builder.header(Header::Authorization, hv);
        }

        if let Some(proxy) = &self.opts.atproto_proxy {
            builder = builder.header(Header::AtprotoProxy, proxy.as_ref());
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

        let body_stream = stream.0.map_ok(|f| f.buffer).boxed();

        let resp = self
            .client
            .send_http_bidirectional(parts, body_stream)
            .await
            .map_err(StreamError::transport)?;

        let (parts, body) = resp.into_parts();

        Ok(XrpcResponseStream::<
            <<S as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<'static>,
        >::from_typed_parts(parts, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[allow(dead_code)]
    struct DummyReq;

    #[derive(Deserialize, Debug, thiserror::Error)]
    #[error("{0}")]
    struct DummyErr<'a>(#[serde(borrow)] CowStr<'a>);

    impl IntoStatic for DummyErr<'_> {
        type Output = DummyErr<'static>;
        fn into_static(self) -> Self::Output {
            DummyErr(self.0.into_static())
        }
    }

    struct DummyResp;

    impl XrpcResp for DummyResp {
        const NSID: &'static str = "test.dummy";
        const ENCODING: &'static str = "application/json";
        type Output<'de> = ();
        type Err<'de> = DummyErr<'de>;
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
        match resp.parse().unwrap_err() {
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
            match resp.parse().unwrap_err() {
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
    fn no_double_slash_in_path() {
        #[derive(Serialize, Deserialize)]
        struct Req;
        #[derive(Deserialize, Debug, thiserror::Error)]
        #[error("{0}")]
        struct Err<'a>(#[serde(borrow)] CowStr<'a>);
        impl IntoStatic for Err<'_> {
            type Output = Err<'static>;
            fn into_static(self) -> Self::Output {
                Err(self.0.into_static())
            }
        }
        struct Resp;
        impl XrpcResp for Resp {
            const NSID: &'static str = "com.example.test";
            const ENCODING: &'static str = "application/json";
            type Output<'de> = ();
            type Err<'de> = Err<'de>;
        }
        impl XrpcRequest for Req {
            const NSID: &'static str = "com.example.test";
            const METHOD: XrpcMethod = XrpcMethod::Query;
            type Response = Resp;
        }

        let opts = CallOptions::default();
        for base in [
            Url::parse("https://pds").unwrap(),
            Url::parse("https://pds/").unwrap(),
            Url::parse("https://pds/base/").unwrap(),
        ] {
            let req = build_http_request(&base, &Req, &opts).unwrap();
            let uri = req.uri().to_string();
            assert!(uri.contains("/xrpc/com.example.test"));
            assert!(!uri.contains("//xrpc"));
        }
    }
}
