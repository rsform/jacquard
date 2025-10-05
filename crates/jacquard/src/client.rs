//! XRPC client implementation for AT Protocol
//!
//! This module provides HTTP and XRPC client traits along with an authenticated
//! client implementation that manages session tokens.

mod at_client;
mod error;
mod response;
mod token;
mod xrpc_call;

use std::fmt::Display;
use std::future::Future;

pub use at_client::{AtClient, SendOverrides};
pub use error::{ClientError, Result};
use http::{
    HeaderName, HeaderValue, Request,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
pub use response::Response;
pub use token::{FileTokenStore, MemoryTokenStore, TokenStore, TokenStoreError};
pub use xrpc_call::{CallOptions, XrpcCall, XrpcExt};

use jacquard_common::{
    CowStr, IntoStatic,
    types::{
        string::{Did, Handle},
        xrpc::{XrpcMethod, XrpcRequest},
    },
};
use url::Url;

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

/// HTTP client trait for sending raw HTTP requests.
pub trait HttpClient {
    /// Error type returned by the HTTP client
    type Error: std::error::Error + Display + Send + Sync + 'static;
    /// Send an HTTP request and return the response.
    fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send;
}
// Note: Stateless and stateful XRPC clients are implemented in xrpc_call.rs and at_client.rs

pub(crate) const NSID_REFRESH_SESSION: &str = "com.atproto.server.refreshSession";

/// Authorization token types for XRPC requests.
#[derive(Debug, Clone)]
pub enum AuthorizationToken<'s> {
    /// Bearer token (access JWT, refresh JWT to refresh the session)
    Bearer(CowStr<'s>),
    /// DPoP token (proof-of-possession) for OAuth
    Dpop(CowStr<'s>),
}

/// Basic client wrapper: reqwest transport + in-memory token store.
pub struct BasicClient(AtClient<reqwest::Client, MemoryTokenStore>);

impl BasicClient {
    /// Construct a basic client with minimal inputs.
    pub fn new(base: Url) -> Self {
        Self(AtClient::new(
            reqwest::Client::new(),
            base,
            MemoryTokenStore::default(),
        ))
    }

    /// Access the inner stateful client.
    pub fn inner(&self) -> &AtClient<reqwest::Client, MemoryTokenStore> {
        &self.0
    }

    /// Send an XRPC request.
    pub async fn send<R: XrpcRequest + Send>(&self, req: R) -> Result<Response<R>> {
        self.0.send(req).await
    }

    /// Send with per-call overrides.
    pub async fn send_with<R: XrpcRequest + Send>(
        &self,
        req: R,
        overrides: SendOverrides<'_>,
    ) -> Result<Response<R>> {
        self.0.send_with(req, overrides).await
    }

    /// Get current session.
    pub async fn session(&self) -> Option<Session> {
        self.0.session().await
    }

    /// Set the session.
    pub async fn set_session(&self, session: Session) -> core::result::Result<(), TokenStoreError> {
        self.0.set_session(session).await
    }

    /// Clear session.
    pub async fn clear_session(&self) -> core::result::Result<(), TokenStoreError> {
        self.0.clear_session().await
    }

    /// Base URL of this client.
    pub fn base(&self) -> &Url {
        self.0.base()
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
pub(crate) fn build_http_request<R: XrpcRequest>(
    base: &Url,
    req: &R,
    opts: &xrpc_call::CallOptions<'_>,
) -> core::result::Result<Request<Vec<u8>>, error::TransportError> {
    let mut url = base.clone();
    let mut path = url.path().trim_end_matches('/').to_owned();
    path.push_str("/xrpc/");
    path.push_str(R::NSID);
    url.set_path(&path);

    if let XrpcMethod::Query = R::METHOD {
        let qs = serde_html_form::to_string(&req)
            .map_err(|e| error::TransportError::InvalidRequest(e.to_string()))?;
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
            error::TransportError::InvalidRequest(format!("Invalid authorization token: {}", e))
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
            .map_err(|e| error::TransportError::InvalidRequest(e.to_string()))?
    } else {
        vec![]
    };

    builder
        .body(body)
        .map_err(|e| error::TransportError::InvalidRequest(e.to_string()))
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

impl From<jacquard_api::com_atproto::server::refresh_session::RefreshSessionOutput<'_>>
    for Session
{
    fn from(
        output: jacquard_api::com_atproto::server::refresh_session::RefreshSessionOutput<'_>,
    ) -> Self {
        Self {
            access_jwt: output.access_jwt.into_static(),
            refresh_jwt: output.refresh_jwt.into_static(),
            did: output.did.into_static(),
            handle: output.handle.into_static(),
        }
    }
}
