//! Stateless XRPC utilities and request/response mapping
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
//!
use bytes::Bytes;
use http::{
    HeaderName, HeaderValue, Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::{self, Debug, Display};
use std::{error::Error, marker::PhantomData};
use url::Url;

use crate::error::TransportError;
use crate::http_client::HttpClient;
use crate::types::value::Data;
use crate::{AuthorizationToken, error::AuthError};
use crate::{CowStr, error::XrpcResult};
use crate::{IntoStatic, error::DecodeError};

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
/// HTTP method, encoding types, and associated output types.
///
/// The trait is implemented on the request parameters/input type itself.
pub trait XrpcRequest<'de>: Serialize + Deserialize<'de> {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;

    /// Output encoding (MIME type)
    const OUTPUT_ENCODING: &'static str;

    /// Response output type
    type Output: Deserialize<'de> + IntoStatic;

    /// Error type for this request
    type Err: Error + Deserialize<'de> + IntoStatic;

    /// Encode the request body for procedures.
    ///
    /// Default implementation serializes to JSON. Override for non-JSON encodings.
    fn encode_body(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Decode the request body for procedures.
    ///
    /// Default implementation deserializes from JSON. Override for non-JSON encodings.
    fn decode_body(&self, body: &'de [u8]) -> Result<Box<Self>, DecodeError> {
        let body: Self = serde_json::from_slice(body).map_err(|e| DecodeError::Json(e))?;

        Ok(Box::new(body))
    }
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
/// ```ignore
/// use jacquard::client::XrpcExt;
/// use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
/// use jacquard::types::ident::AtIdentifier;
/// use miette::IntoDiagnostic;
///
/// #[tokio::main]
/// async fn main() -> miette::Result<()> {
///     let http = reqwest::Client::new();
///     let base = url::Url::parse("https://public.api.bsky.app")?;
///     let resp = http
///         .xrpc(base)
///         .send(
///             GetAuthorFeed::new()
///                 .actor(AtIdentifier::new_static("pattern.atproto.systems").unwrap())
///                 .limit(5)
///                 .build(),
///         )
///         .await?;
///     let out = resp.into_output()?;
///     println!("author feed:\n{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
///     Ok(())
/// }
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

/// Stateless XRPC call builder.
///
/// Example (per-request overrides)
/// ```ignore
/// use jacquard::client::{XrpcExt, AuthorizationToken};
/// use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
/// use jacquard::types::ident::AtIdentifier;
/// use jacquard::CowStr;
/// use miette::IntoDiagnostic;
///
/// #[tokio::main]
/// async fn main() -> miette::Result<()> {
///     let http = reqwest::Client::new();
///     let base = url::Url::parse("https://public.api.bsky.app")?;
///     let resp = http
///         .xrpc(base)
///         .auth(AuthorizationToken::Bearer(CowStr::from("ACCESS_JWT")))
///         .accept_labelers(vec![CowStr::from("did:plc:labelerid")])
///         .header(http::header::USER_AGENT, http::HeaderValue::from_static("jacquard-example"))
///         .send(
///             GetAuthorFeed::new()
///                 .actor(AtIdentifier::new_static("pattern.atproto.systems").unwrap())
///                 .limit(5)
///                 .build(),
///         )
///         .await?;
///     let out = resp.into_output()?;
///     println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
///     Ok(())
/// }
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
    pub async fn send<'de, R>(self, request: &'de R) -> XrpcResult<Response<'de, R>>
    where
        R: XrpcRequest<'de> + Send,
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

    /// Send the given typed XRPC request and return an owned response wrapper.
    ///
    /// Note on 401 handling:
    /// - When the server returns 401 with a `WWW-Authenticate` header, this surfaces as
    ///   `ClientError::Auth(AuthError::Other(header))` so higher layers (e.g., OAuth/DPoP) can
    ///   inspect the header for `error="invalid_token"` or `error="use_dpop_nonce"` and react
    ///   (refresh/retry). If the header is absent, the 401 body flows through to `Response` and
    ///   can be parsed/mapped to `AuthError` as appropriate.
    pub async fn send_owned<'de, R, S>(self, request: R) -> XrpcResult<OwnedResponse<S>>
    where
        R: XrpcRequest<'de> + Send + 'de,
        S: XrpcRequest<'static>,
    {
        let http_request = build_http_request(&self.base, &request, &self.opts)
            .map_err(crate::error::TransportError::from)?;

        let http_response = self
            .client
            .send_http(http_request)
            .await
            .map_err(|e| crate::error::TransportError::Other(Box::new(e)))?;

        let resp: XrpcResult<Response<'de, R>> = process_response(http_response);
        match resp {
            Ok(response) => Ok(response.owned()),
            Err(err) => Err(err),
        }
    }
}

/// Process the HTTP response from the server into a proper xrpc response statelessly.
///
/// Exposed to make things more easily pluggable
#[inline]
pub fn process_response<'de, R: XrpcRequest<'de> + Send>(
    http_response: http::Response<Vec<u8>>,
) -> XrpcResult<Response<'de, R>> {
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
pub fn build_http_request<'de, R: XrpcRequest<'de>>(
    base: &Url,
    req: &R,
    opts: &CallOptions<'_>,
) -> core::result::Result<Request<Vec<u8>>, crate::error::TransportError> {
    let mut url = base.clone();
    let mut path = url.path().trim_end_matches('/').to_owned();
    path.push_str("/xrpc/");
    path.push_str(R::NSID);
    url.set_path(&path);

    if let XrpcMethod::Query = R::METHOD {
        let qs = serde_html_form::to_string(&req)
            .map_err(|e| crate::error::TransportError::InvalidRequest(e.to_string()))?;
        if !qs.is_empty() {
            url.set_query(Some(&qs));
        } else {
            url.set_query(None);
        }
    }

    let method = match R::METHOD {
        XrpcMethod::Query => http::Method::GET,
        XrpcMethod::Procedure(_) => http::Method::POST,
    };

    let mut builder = Request::builder().method(method).uri(url.as_str());

    if let XrpcMethod::Procedure(encoding) = R::METHOD {
        builder = builder.header(Header::ContentType, encoding);
    }
    builder = builder.header(http::header::ACCEPT, R::OUTPUT_ENCODING);

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
pub struct Response<'de, R: XrpcRequest<'de>> {
    buffer: Bytes,
    status: StatusCode,
    _marker: PhantomData<&'de R>,
}

/// Owned response wrapper which allows taking ownership of the resulting response type
pub struct OwnedResponse<R: XrpcRequest<'static>> {
    buffer: Bytes,
    status: StatusCode,
    _marker: PhantomData<R>,
}

impl<'de, R: XrpcRequest<'de>> Response<'de, R> {
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

    /// Parse the response, borrowing from the internal buffer
    pub fn parse(&'de self) -> Result<R::Output, XrpcError<R::Err>> {
        // Use a helper to make lifetime inference work
        fn parse_output<'b, R: XrpcRequest<'b>>(
            buffer: &'b [u8],
        ) -> Result<R::Output, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        fn parse_error<'b, R: XrpcRequest<'b>>(
            buffer: &'b [u8],
        ) -> Result<R::Err, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match parse_output::<'de, R>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match parse_error::<'de, R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = R::METHOD.as_str();
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => {
                                    Err(XrpcError::<R::Err>::Auth(AuthError::TokenExpired))
                                }
                                "InvalidToken" => {
                                    Err(XrpcError::<R::Err>::Auth(AuthError::InvalidToken))
                                }
                                _ => Err(XrpcError::Generic(generic)),
                            }
                        }
                        Err(e) => Err(XrpcError::Decode(e)),
                    }
                }
            }
        // 401: always auth error
        } else {
            match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(mut generic) => {
                    generic.nsid = R::NSID;
                    generic.method = R::METHOD.as_str();
                    generic.http_status = self.status;
                    match generic.error.as_str() {
                        "ExpiredToken" => Err(XrpcError::<R::Err>::Auth(AuthError::TokenExpired)),
                        "InvalidToken" => Err(XrpcError::<R::Err>::Auth(AuthError::InvalidToken)),
                        _ => Err(XrpcError::<R::Err>::Auth(AuthError::NotAuthenticated)),
                    }
                }
                Err(e) => Err(XrpcError::Decode(e)),
            }
        }
    }

    /// Get the raw buffer
    pub fn buffer(&self) -> &Bytes {
        &self.buffer
    }

    /// Take ownership of the response so the original can be dropped
    pub fn owned<S>(&self) -> OwnedResponse<S>
    where
        S: XrpcRequest<'static>,
    {
        OwnedResponse {
            buffer: self.buffer.clone(),
            status: self.status,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<R: XrpcRequest<'static>> OwnedResponse<R> {
    /// Parse the response into an owned output
    pub fn output(self) -> Result<R::Output, XrpcError<R::Err>>
    where
        for<'a> R::Output: IntoStatic<Output = R::Output>,
        for<'a> R::Err: IntoStatic<Output = R::Err>,
    {
        // Use a helper to make lifetime inference work
        fn parse_output<'b, R: XrpcRequest<'b>>(
            buffer: Box<[u8]>,
        ) -> Result<R::Output, serde_json::Error> {
            let buf: &'static [u8] = Box::leak(buffer);
            serde_json::from_slice(buf)
        }

        fn parse_error<'b, R: XrpcRequest<'b>>(
            buffer: Box<[u8]>,
        ) -> Result<R::Err, serde_json::Error> {
            let buf: &'static [u8] = Box::leak(buffer);
            serde_json::from_slice(buf)
        }

        // 200: parse as output
        if self.status.is_success() {
            match parse_output::<'static, R>(Box::from(self.buffer.as_ref())) {
                Ok(output) => Ok(output.into_static()),
                Err(e) => Err(XrpcError::<R::Err>::Decode(e).into_static()),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            let result = match parse_error::<'static, R>(Box::from(self.buffer.as_ref())) {
                Ok(error) => XrpcError::Xrpc(error),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(mut generic) => {
                            generic.nsid = R::NSID;
                            generic.method = R::METHOD.as_str();
                            generic.http_status = self.status;
                            // Map auth-related errors to AuthError
                            match generic.error.as_ref() {
                                "ExpiredToken" => {
                                    XrpcError::<R::Err>::Auth(AuthError::TokenExpired)
                                }
                                "InvalidToken" => {
                                    XrpcError::<R::Err>::Auth(AuthError::InvalidToken)
                                }
                                _ => XrpcError::Generic(generic),
                            }
                        }
                        Err(e) => XrpcError::Decode(e),
                    }
                }
            };
            Err(result.into_static())
        // 401: always auth error
        } else {
            let result = match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(mut generic) => {
                    let status = self.status;
                    generic.nsid = R::NSID;
                    generic.method = R::METHOD.as_str();
                    generic.http_status = status;
                    match generic.error.as_ref() {
                        "ExpiredToken" => XrpcError::<R::Err>::Auth(AuthError::TokenExpired),
                        "InvalidToken" => XrpcError::<R::Err>::Auth(AuthError::InvalidToken),
                        _ => XrpcError::<R::Err>::Auth(AuthError::NotAuthenticated),
                    }
                }
                Err(e) => XrpcError::Decode(e),
            };
            Err(result.into_static())
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
pub enum XrpcError<E: Debug + Display + IntoStatic> {
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
    Decode(#[from] serde_json::Error),
}

impl<E: std::error::Error + IntoStatic> IntoStatic for XrpcError<E>
where
    <E as crate::into_static::IntoStatic>::Output: IntoStatic,
    <E as crate::into_static::IntoStatic>::Output: Debug + Display,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
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

    impl<'de> XrpcRequest<'de> for DummyReq {
        const NSID: &'static str = "test.dummy";
        const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
        const OUTPUT_ENCODING: &'static str = "application/json";
        type Output = ();
        type Err = DummyErr<'de>;
    }

    #[test]
    fn generic_error_carries_context() {
        let body = serde_json::json!({"error":"InvalidRequest","message":"missing"});
        let buf = Bytes::from(serde_json::to_vec(&body).unwrap());
        let resp: Response<DummyReq> = Response::new(buf, StatusCode::BAD_REQUEST);
        match resp.parse().unwrap_err() {
            XrpcError::Generic(g) => {
                assert_eq!(g.error.as_str(), "InvalidRequest");
                assert_eq!(g.message.as_deref(), Some("missing"));
                assert_eq!(g.nsid, DummyReq::NSID);
                assert_eq!(g.method, DummyReq::METHOD.as_str());
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
            let resp: Response<DummyReq> = Response::new(buf, StatusCode::UNAUTHORIZED);
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
        impl<'de> XrpcRequest<'de> for Req {
            const NSID: &'static str = "com.example.test";
            const METHOD: XrpcMethod = XrpcMethod::Query;
            const OUTPUT_ENCODING: &'static str = "application/json";
            type Output = ();
            type Err = Err<'de>;
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

/// Stateful XRPC call trait
pub trait XrpcClient: HttpClient {
    /// Get the base URI for the client.
    fn base_uri(&self) -> Url;

    /// Get the call options for the client.
    fn opts(&self) -> impl Future<Output = CallOptions<'_>> {
        async { CallOptions::default() }
    }
    /// Send an XRPC request and parse the response
    fn send<'de, R: XrpcRequest<'de> + Send>(
        &self,
        request: &'de R,
    ) -> impl Future<Output = XrpcResult<Response<'de, R>>>;
}
