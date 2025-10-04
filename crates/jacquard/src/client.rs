//! XRPC client implementation for AT Protocol
//!
//! This module provides HTTP and XRPC client traits along with an authenticated
//! client implementation that manages session tokens.

mod error;
mod response;

use std::fmt::Display;
use std::future::Future;

use bytes::Bytes;
pub use error::{ClientError, Result};
use http::{
    HeaderName, HeaderValue, Request,
    header::{AUTHORIZATION, CONTENT_TYPE, InvalidHeaderValue},
};
pub use response::Response;

use jacquard_common::{
    CowStr, IntoStatic,
    types::{
        string::{Did, Handle},
        xrpc::{XrpcMethod, XrpcRequest},
    },
};

/// Implement HttpClient for reqwest::Client
impl HttpClient for reqwest::Client {
    type Error = reqwest::Error;

    async fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<Vec<u8>>, Self::Error> {
        // Convert http::Request to reqwest::Request
        let (parts, body) = request.into_parts();

        let mut req = self.request(parts.method, parts.uri.to_string()).body(body);

        // Copy headers
        for (name, value) in parts.headers.iter() {
            req = req.header(name.as_str(), value.as_bytes());
        }

        // Send request
        let resp = req.send().await?;

        // Convert reqwest::Response to http::Response
        let mut builder = http::Response::builder().status(resp.status());

        // Copy headers
        for (name, value) in resp.headers().iter() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }

        // Read body
        let body = resp.bytes().await?.to_vec();

        Ok(builder.body(body).expect("Failed to build response"))
    }
}

/// HTTP client trait for sending raw HTTP requests
pub trait HttpClient {
    /// Error type returned by the HTTP client
    type Error: std::error::Error + Display + Send + Sync + 'static;
    /// Send an HTTP request and return the response.
    fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send;
}
/// XRPC client trait for AT Protocol RPC calls
pub trait XrpcClient: HttpClient + Sync {
    /// Get the base URI for XRPC requests (e.g., "https://bsky.social")
    fn base_uri(&self) -> CowStr<'_>;
    /// Get the authorization token for XRPC requests
    #[allow(unused_variables)]
    fn authorization_token(
        &self,
        is_refresh: bool,
    ) -> impl Future<Output = Option<AuthorizationToken<'_>>> + Send {
        async { None }
    }
    /// Get the `atproto-proxy` header.
    fn atproto_proxy_header(&self) -> impl Future<Output = Option<String>> + Send {
        async { None }
    }
    /// Get the `atproto-accept-labelers` header.
    fn atproto_accept_labelers_header(&self) -> impl Future<Output = Option<Vec<String>>> + Send {
        async { None }
    }
    /// Send an XRPC request and get back a response
    fn send<R: XrpcRequest + Send>(&self, request: R) -> impl Future<Output = Result<Response<R>>> + Send
    where
        Self: Sized + Sync,
    {
        send_xrpc(self, request)
    }
}

pub(crate) const NSID_REFRESH_SESSION: &str = "com.atproto.server.refreshSession";

/// Authorization token types for XRPC requests
pub enum AuthorizationToken<'s> {
    /// Bearer token (access JWT, refresh JWT to refresh the session)
    Bearer(CowStr<'s>),
    /// DPoP token (proof-of-possession) for OAuth
    Dpop(CowStr<'s>),
}

impl TryFrom<AuthorizationToken<'_>> for HeaderValue {
    type Error = InvalidHeaderValue;

    fn try_from(token: AuthorizationToken) -> core::result::Result<Self, Self::Error> {
        HeaderValue::from_str(&match token {
            AuthorizationToken::Bearer(t) => format!("Bearer {t}"),
            AuthorizationToken::Dpop(t) => format!("DPoP {t}"),
        })
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

/// Generic XRPC send implementation that uses HttpClient
async fn send_xrpc<R, C>(client: &C, request: R) -> Result<Response<R>>
where
    R: XrpcRequest + Send,
    C: XrpcClient + ?Sized + Sync,
{
    // Build URI: base_uri + /xrpc/ + NSID
    let mut uri = format!("{}/xrpc/{}", client.base_uri(), R::NSID);

    // Add query parameters for Query methods
    if let XrpcMethod::Query = R::METHOD {
        let qs = serde_html_form::to_string(&request).map_err(error::EncodeError::from)?;
        if !qs.is_empty() {
            uri.push('?');
            uri.push_str(&qs);
        }
    }

    // Build HTTP request
    let method = match R::METHOD {
        XrpcMethod::Query => http::Method::GET,
        XrpcMethod::Procedure(_) => http::Method::POST,
    };

    let mut builder = Request::builder().method(method).uri(&uri);

    // Add Content-Type for procedures
    if let XrpcMethod::Procedure(encoding) = R::METHOD {
        builder = builder.header(Header::ContentType, encoding);
    }

    // Add authorization header
    let is_refresh = R::NSID == NSID_REFRESH_SESSION;
    if let Some(token) = client.authorization_token(is_refresh).await {
        let header_value: HeaderValue = token.try_into().map_err(|e| {
            error::TransportError::InvalidRequest(format!("Invalid authorization token: {}", e))
        })?;
        builder = builder.header(Header::Authorization, header_value);
    }

    // Add atproto-proxy header
    if let Some(proxy) = client.atproto_proxy_header().await {
        builder = builder.header(Header::AtprotoProxy, proxy);
    }

    // Add atproto-accept-labelers header
    if let Some(labelers) = client.atproto_accept_labelers_header().await {
        builder = builder.header(Header::AtprotoAcceptLabelers, labelers.join(", "));
    }

    // Serialize body for procedures
    let body = if let XrpcMethod::Procedure(_) = R::METHOD {
        request.encode_body()?
    } else {
        vec![]
    };

    // TODO: make this not panic
    let http_request = builder.body(body).expect("Failed to build HTTP request");

    // Send HTTP request
    let http_response = client
        .send_http(http_request)
        .await
        .map_err(|e| error::TransportError::Other(Box::new(e)))?;

    let status = http_response.status();
    let buffer = Bytes::from(http_response.into_body());

    // XRPC errors come as 400/401 with structured error bodies
    // Other error status codes (404, 500, etc.) are generic HTTP errors
    if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
        return Err(ClientError::Http(error::HttpError {
            status,
            body: Some(buffer),
        }));
    }

    // Response will parse XRPC errors for 400/401, or output for 2xx
    Ok(Response::new(buffer, status))
}

/// Session information from `com.atproto.server.createSession`
///
/// Contains the access and refresh tokens along with user identity information.
#[derive(Debug, Clone)]
pub struct Session {
    /// Access token (JWT) used for authenticated requests
    pub access_jwt: CowStr<'static>,
    /// Refresh token (JWT) used to obtain new access tokens
    pub refresh_jwt: CowStr<'static>,
    /// User's DID (Decentralized Identifier)
    pub did: Did<'static>,
    /// User's handle (e.g., "alice.bsky.social")
    pub handle: Handle<'static>,
}

impl From<jacquard_api::com_atproto::server::create_session::CreateSessionOutput<'_>> for Session {
    fn from(
        output: jacquard_api::com_atproto::server::create_session::CreateSessionOutput<'_>,
    ) -> Self {
        Self {
            access_jwt: output.access_jwt.into_static(),
            refresh_jwt: output.refresh_jwt.into_static(),
            did: output.did.into_static(),
            handle: output.handle.into_static(),
        }
    }
}

/// Authenticated XRPC client wrapper that manages session tokens
///
/// Wraps an HTTP client and adds automatic Bearer token authentication for XRPC requests.
/// Handles both access tokens for regular requests and refresh tokens for session refresh.
pub struct AuthenticatedClient<C> {
    client: C,
    base_uri: CowStr<'static>,
    session: Option<Session>,
}

impl<C> AuthenticatedClient<C> {
    /// Create a new authenticated client with a base URI
    ///
    /// # Example
    /// ```ignore
    /// let client = AuthenticatedClient::new(
    ///     reqwest::Client::new(),
    ///     CowStr::from("https://bsky.social")
    /// );
    /// ```
    pub fn new(client: C, base_uri: CowStr<'static>) -> Self {
        Self {
            client,
            base_uri: base_uri,
            session: None,
        }
    }

    /// Set the session obtained from `createSession` or `refreshSession`
    pub fn set_session(&mut self, session: Session) {
        self.session = Some(session);
    }

    /// Get the current session if one exists
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Clear the current session locally
    ///
    /// Note: This only clears the local session state. To properly revoke the session
    /// server-side, use `com.atproto.server.deleteSession` before calling this.
    pub fn clear_session(&mut self) {
        self.session = None;
    }
}

impl<C: HttpClient> HttpClient for AuthenticatedClient<C> {
    type Error = C::Error;

    fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> {
        self.client.send_http(request)
    }
}

impl<C: HttpClient + Sync> XrpcClient for AuthenticatedClient<C> {
    fn base_uri(&self) -> CowStr<'_> {
        self.base_uri.clone()
    }

    async fn authorization_token(&self, is_refresh: bool) -> Option<AuthorizationToken<'_>> {
        if is_refresh {
            self.session
                .as_ref()
                .map(|s| AuthorizationToken::Bearer(s.refresh_jwt.clone()))
        } else {
            self.session
                .as_ref()
                .map(|s| AuthorizationToken::Bearer(s.access_jwt.clone()))
        }
    }
}
