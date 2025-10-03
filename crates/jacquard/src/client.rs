mod error;
mod response;

use std::fmt::Display;
use std::future::Future;

pub use error::{ClientError, Result};
use bytes::Bytes;
use http::{
    HeaderName, HeaderValue, Request,
    header::{AUTHORIZATION, CONTENT_TYPE, InvalidHeaderValue},
};
pub use response::Response;
use serde::Serialize;

use jacquard_common::{CowStr, types::xrpc::{XrpcMethod, XrpcRequest}};

pub trait HttpClient {
    type Error: std::error::Error + Display + Send + Sync + 'static;
    /// Send an HTTP request and return the response.
    fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>>;
}
/// XRPC client trait
pub trait XrpcClient: HttpClient {
    fn base_uri(&self) -> CowStr<'_>;
    #[allow(unused_variables)]
    fn authorization_token(
        &self,
        is_refresh: bool,
    ) -> impl Future<Output = Option<AuthorizationToken<'_>>> {
        async { None }
    }
    /// Get the `atproto-proxy` header.
    fn atproto_proxy_header(&self) -> impl Future<Output = Option<String>> {
        async { None }
    }
    /// Get the `atproto-accept-labelers` header.
    fn atproto_accept_labelers_header(&self) -> impl Future<Output = Option<Vec<String>>> {
        async { None }
    }
    /// Send an XRPC request and get back a response
    fn send<R: XrpcRequest>(&self, request: R) -> impl Future<Output = Result<Response<R>>>
    where
        Self: Sized,
    {
        send_xrpc(self, request)
    }
}

pub(crate) const NSID_REFRESH_SESSION: &str = "com.atproto.server.refreshSession";

pub enum AuthorizationToken<'s> {
    Bearer(CowStr<'s>),
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

/// HTTP headers which can be used in XPRC requests.
pub enum Header {
    ContentType,
    Authorization,
    AtprotoProxy,
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
    R: XrpcRequest,
    C: XrpcClient + ?Sized,
{
    // Build URI: base_uri + /xrpc/ + NSID
    let mut uri = format!("{}/xrpc/{}", client.base_uri(), R::NSID);

    // Add query parameters for Query methods
    if let XrpcMethod::Query = R::METHOD {
        if let Ok(qs) = serde_html_form::to_string(&request) {
            if !qs.is_empty() {
                uri.push('?');
                uri.push_str(&qs);
            }
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
    let body = if let XrpcMethod::Procedure(encoding) = R::METHOD {
        if encoding == "application/json" {
            serde_json::to_vec(&request).map_err(error::EncodeError::Json)?
        } else {
            // For other encodings, we'd need different serialization
            vec![]
        }
    } else {
        vec![]
    };

    let http_request = builder.body(body).expect("Failed to build HTTP request");

    // Send HTTP request
    let http_response = client.send_http(http_request).await.map_err(|e| {
        error::TransportError::Other(Box::new(e))
    })?;

    // Check status
    if !http_response.status().is_success() {
        return Err(ClientError::Http(error::HttpError {
            status: http_response.status(),
            body: Some(Bytes::from(http_response.body().clone())),
        }));
    }

    // Convert to Response
    let buffer = Bytes::from(http_response.into_body());
    Ok(Response::new(buffer))
}
