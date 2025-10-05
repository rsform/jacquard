use bytes::Bytes;
use http::{
    HeaderName, HeaderValue, Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::{self, Debug};
use std::{error::Error, marker::PhantomData};
use url::Url;

use crate::IntoStatic;
use crate::error::TransportError;
use crate::http_client::HttpClient;
use crate::types::value::Data;
use crate::{AuthorizationToken, error::AuthError};
use crate::{CowStr, error::XrpcResult};

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
pub trait XrpcRequest: Serialize {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;

    /// Output encoding (MIME type)
    const OUTPUT_ENCODING: &'static str;

    /// Response output type
    type Output<'de>: Deserialize<'de> + IntoStatic;

    /// Error type for this request
    type Err<'de>: Error + Deserialize<'de> + IntoStatic;

    /// Encode the request body for procedures.
    ///
    /// Default implementation serializes to JSON. Override for non-JSON encodings.
    fn encode_body(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(serde_json::to_vec(self)?)
    }
}

/// Error type for XRPC endpoints that don't define any errors
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct GenericError<'a>(Data<'a>);

impl fmt::Display for GenericError<'_> {
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
    pub async fn send<R: XrpcRequest + Send>(self, request: R) -> XrpcResult<Response<R>> {
        let http_request = build_http_request(&self.base, &request, &self.opts)
            .map_err(crate::error::TransportError::from)?;

        let http_response = self
            .client
            .send_http(http_request)
            .await
            .map_err(|e| crate::error::TransportError::Other(Box::new(e)))?;

        let status = http_response.status();
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
pub fn build_http_request<R: XrpcRequest>(
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
/// Supports both borrowed parsing (with `parse()`) and owned parsing (with `into_output()`).
pub struct Response<R: XrpcRequest> {
    buffer: Bytes,
    status: StatusCode,
    _marker: PhantomData<R>,
}

impl<R: XrpcRequest> Response<R> {
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
    pub fn parse(&self) -> Result<R::Output<'_>, XrpcError<R::Err<'_>>> {
        // Use a helper to make lifetime inference work
        fn parse_output<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Output<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        fn parse_error<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Err<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match parse_output::<R>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match parse_error::<R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(generic) => {
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
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
                Ok(generic) => match generic.error.as_str() {
                    "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                    "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                    _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                },
                Err(e) => Err(XrpcError::Decode(e)),
            }
        }
    }

    /// Parse the response into an owned output
    pub fn into_output(self) -> Result<R::Output<'static>, XrpcError<R::Err<'static>>>
    where
        for<'a> R::Output<'a>: IntoStatic<Output = R::Output<'static>>,
        for<'a> R::Err<'a>: IntoStatic<Output = R::Err<'static>>,
    {
        // Use a helper to make lifetime inference work
        fn parse_output<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Output<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        fn parse_error<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Err<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match parse_output::<R>(&self.buffer) {
                Ok(output) => Ok(output.into_static()),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match parse_error::<R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error.into_static())),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(generic) => {
                            // Map auth-related errors to AuthError
                            match generic.error.as_ref() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
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
                Ok(generic) => match generic.error.as_ref() {
                    "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                    "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                    _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                },
                Err(e) => Err(XrpcError::Decode(e)),
            }
        }
    }

    /// Get the raw buffer
    pub fn buffer(&self) -> &Bytes {
        &self.buffer
    }
}

/// Generic XRPC error format for untyped errors like InvalidRequest
///
/// Used when the error doesn't match the endpoint's specific error enum
#[derive(Debug, Clone, Deserialize)]
pub struct GenericXrpcError {
    /// Error code (e.g., "InvalidRequest")
    pub error: SmolStr,
    /// Optional error message with details
    pub message: Option<SmolStr>,
}

impl std::fmt::Display for GenericXrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(msg) = &self.message {
            write!(f, "{}: {}", self.error, msg)
        } else {
            write!(f, "{}", self.error)
        }
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
    Xrpc(E),

    /// Authentication error (ExpiredToken, InvalidToken, etc.)
    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    /// Generic XRPC error not in the endpoint's error enum (e.g., InvalidRequest)
    #[error("XRPC error: {0}")]
    Generic(GenericXrpcError),

    /// Failed to decode the response body
    #[error("Failed to decode response: {0}")]
    Decode(#[from] serde_json::Error),
}
