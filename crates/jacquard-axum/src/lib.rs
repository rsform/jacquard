//! # Axum helpers for jacquard XRPC server implementations.
//!
//! ## Usage.
//!
//! This crate provides server-side helpers for wiring generated Jacquard XRPC
//! endpoint marker types into axum routers. [`ExtractXrpc`] decodes query
//! parameters or procedure bodies into owned request values, defaulting to
//! `DefaultStr`-backed generated types, and [`XrpcResponse`] encodes endpoint
//! outputs with the content type declared by the endpoint response marker.
//!
//! ```no_run
//! use axum::Router;
//! use jacquard::api::com_atproto::identity::resolve_handle::{
//!     ResolveHandleOutput, ResolveHandleRequest,
//! };
//! use jacquard::types::string::Did;
//! use jacquard_axum::{ExtractXrpc, IntoRouter, XrpcResponse};
//! use miette::{IntoDiagnostic, Result};
//!
//! async fn handle_resolve(
//!     ExtractXrpc(req): ExtractXrpc<ResolveHandleRequest>,
//! ) -> XrpcResponse<ResolveHandleRequest> {
//!     let _handle = req.handle;
//!     XrpcResponse(ResolveHandleOutput {
//!         did: Did::new_static("did:plc:test").unwrap(),
//!         extra_data: None,
//!     })
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let app = Router::new()
//!         .route("/", axum::routing::get(|| async { "hello world!" }))
//!         .merge(ResolveHandleRequest::into_router(handle_resolve));
//!
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
//!         .await
//!         .into_diagnostic()?;
//!     axum::serve(listener, app).await.unwrap();
//!     Ok(())
//! }
//! ```
//!
//! The extractor uses the [`XrpcEndpoint`] trait to determine request type:
//!
//! - Query endpoints deserialize from query string parameters.
//! - Procedure endpoints deserialize from request bodies and preserve custom
//!   encodings through [`XrpcRequest::decode_body`].
//!
//! Deserialization errors return a 400 Bad Request with a JSON body matching the
//! XRPC error format.
//!
//! [`IntoRouter`] is implemented for endpoint marker types. The endpoint marker
//! receiver style, such as `ResolveHandleRequest::into_router(handle_resolve)`,
//! keeps routing no-turbofish while still deriving the path and HTTP method from
//! [`XrpcEndpoint`].

pub mod did_web;
pub mod oauth;
#[cfg(feature = "service-auth")]
pub mod service_auth;

use std::borrow::Cow;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use jacquard::{
    BosStr, CowStr, DefaultStr, IntoStatic,
    xrpc::{XrpcEndpoint, XrpcError, XrpcMethod, XrpcRequest, XrpcResp},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

/// Axum extractor for XRPC requests.
///
/// Deserializes incoming requests based on the endpoint's method type and
/// returns the request type ready for handler logic.
pub struct ExtractXrpc<R: XrpcEndpoint, B: BosStr = DefaultStr>(pub R::Request<B>);

impl<R, B, State> FromRequest<State> for ExtractXrpc<R, B>
where
    R: XrpcEndpoint,
    B: XrpcExtractBacking<R>,
    State: Send + Sync,
{
    type Rejection = Response;

    fn from_request(
        req: Request,
        state: &State,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async {
            match R::METHOD {
                XrpcMethod::Procedure(_) => {
                    let body = Bytes::from_request(req, state)
                        .await
                        .map_err(IntoResponse::into_response)?;
                    B::decode_body(&body).map(ExtractXrpc)
                }
                XrpcMethod::Query => {
                    let query = req.uri().query().unwrap_or("");
                    B::decode_query(query).map(ExtractXrpc)
                }
            }
        }
    }
}

/// Backing-specific XRPC request extraction policy.
///
/// Implementations choose the decode backing separately from the handler-visible
/// backing to avoid overlapping axum extractor impls.
pub trait XrpcExtractBacking<R>: private::Sealed + BosStr + Sized + 'static
where
    R: XrpcEndpoint,
{
    /// Decodes a query request into this handler-visible backing.
    fn decode_query(query: &str) -> Result<R::Request<Self>, Response>;

    /// Decodes a procedure body into this handler-visible backing.
    fn decode_body(body: &[u8]) -> Result<R::Request<Self>, Response>;
}

macro_rules! impl_owned_extract_backing {
    ($backing:ty) => {
        impl private::Sealed for $backing {}

        impl<R> XrpcExtractBacking<R> for $backing
        where
            R: XrpcEndpoint,
            R::Request<Self>: DeserializeOwned,
        {
            fn decode_query(query: &str) -> Result<R::Request<Self>, Response> {
                serde_html_form::from_str::<R::Request<Self>>(query)
                    .map_err(|err| invalid_request(format!("failed to decode request: {err}")))
            }

            fn decode_body(body: &[u8]) -> Result<R::Request<Self>, Response> {
                <R::Request<Self> as XrpcRequest>::decode_body(body)
                    .map_err(|err| invalid_request(format!("failed to decode request: {err}")))
            }
        }
    };
}

impl_owned_extract_backing!(DefaultStr);
impl_owned_extract_backing!(String);

impl private::Sealed for CowStr<'static> {}

impl<R> XrpcExtractBacking<R> for CowStr<'static>
where
    R: XrpcEndpoint,
    for<'de> R::Request<CowStr<'de>>: Deserialize<'de>,
    for<'a> R::Request<CowStr<'a>>: IntoStatic<Output = R::Request<CowStr<'static>>>,
{
    fn decode_query(query: &str) -> Result<R::Request<Self>, Response> {
        serde_html_form::from_str::<R::Request<CowStr<'_>>>(query)
            .map(IntoStatic::into_static)
            .map_err(|err| invalid_request(format!("failed to decode request: {err}")))
    }

    fn decode_body(body: &[u8]) -> Result<R::Request<Self>, Response> {
        <R::Request<CowStr<'_>> as XrpcRequest>::decode_body(body)
            .map(IntoStatic::into_static)
            .map_err(|err| invalid_request(format!("failed to decode request: {err}")))
    }
}

impl private::Sealed for Cow<'static, str> {}

impl<R> XrpcExtractBacking<R> for Cow<'static, str>
where
    R: XrpcEndpoint,
    for<'de> R::Request<Cow<'de, str>>: Deserialize<'de>,
    for<'a> R::Request<Cow<'a, str>>: IntoStatic<Output = R::Request<Cow<'static, str>>>,
{
    fn decode_query(query: &str) -> Result<R::Request<Self>, Response> {
        serde_html_form::from_str::<R::Request<Cow<'_, str>>>(query)
            .map(IntoStatic::into_static)
            .map_err(|err| invalid_request(format!("failed to decode request: {err}")))
    }

    fn decode_body(body: &[u8]) -> Result<R::Request<Self>, Response> {
        <R::Request<Cow<'_, str>> as XrpcRequest>::decode_body(body)
            .map(IntoStatic::into_static)
            .map_err(|err| invalid_request(format!("failed to decode request: {err}")))
    }
}

mod private {
    pub trait Sealed {}
}

/// Typed axum response wrapper for XRPC endpoint outputs.
pub struct XrpcResponse<R: XrpcEndpoint, B: BosStr = DefaultStr>(
    pub <R::Response as XrpcResp>::Output<B>,
);

impl<R, B> XrpcResponse<R, B>
where
    R: XrpcEndpoint,
    B: BosStr,
{
    /// Creates a typed XRPC response from an endpoint output value.
    pub fn new(value: <R::Response as XrpcResp>::Output<B>) -> Self {
        Self(value)
    }
}

impl<R, B> IntoResponse for XrpcResponse<R, B>
where
    R: XrpcEndpoint,
    B: BosStr,
    <R::Response as XrpcResp>::Output<B>: serde::Serialize,
{
    fn into_response(self) -> Response {
        match <R::Response as XrpcResp>::encode_output(&self.0) {
            Ok(body) => (
                StatusCode::OK,
                [(CONTENT_TYPE, <R::Response as XrpcResp>::ENCODING)],
                body,
            )
                .into_response(),
            Err(err) => internal_server_error_response(format!("failed to encode response: {err}")),
        }
    }
}

/// Conversion trait to turn an XRPC endpoint marker and a handler into a router.
///
/// This trait is implemented for endpoint marker types (`R: XrpcEndpoint`). It
/// registers `R::PATH` and maps `R::METHOD` to GET for query endpoints or POST
/// for procedure endpoints. The endpoint-associated call style intentionally
/// avoids turbofish while keeping the endpoint type explicit to the compiler.
pub trait IntoRouter {
    /// Creates an axum router that invokes `handler` for this endpoint marker.
    fn into_router<HandlerArgs, State, Handler>(handler: Handler) -> Router<State>
    where
        HandlerArgs: 'static,
        State: Clone + Send + Sync + 'static,
        Handler: axum::handler::Handler<HandlerArgs, State>;
}

impl<R> IntoRouter for R
where
    R: XrpcEndpoint,
{
    fn into_router<HandlerArgs, State, Handler>(handler: Handler) -> Router<State>
    where
        HandlerArgs: 'static,
        State: Clone + Send + Sync + 'static,
        Handler: axum::handler::Handler<HandlerArgs, State>,
    {
        Router::new().route(
            R::PATH,
            (match R::METHOD {
                XrpcMethod::Query => axum::routing::get,
                XrpcMethod::Procedure(_) => axum::routing::post,
            })(handler),
        )
    }
}

/// Axum-compatible generic XRPC error response.
///
/// Use this type when no generated endpoint error value is available, such as
/// local infrastructure failures in a handler.
#[derive(Debug, Clone)]
pub struct GenericXrpcErrorResponse {
    pub status: StatusCode,
    error: String,
    message: Option<String>,
}

impl GenericXrpcErrorResponse {
    /// Creates an internal server error response.
    pub fn internal_server_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalServerError",
            Some("internal server error"),
        )
    }

    /// Creates a generic XRPC error response with a custom message.
    pub fn internal_server_error_with_message(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalServerError",
            Some(message),
        )
    }

    /// Creates a generic XRPC error response.
    pub fn new(
        status: StatusCode,
        error: impl Into<String>,
        message: Option<impl Into<String>>,
    ) -> Self {
        Self {
            status,
            error: error.into(),
            message: message.map(Into::into),
        }
    }
}

impl IntoResponse for GenericXrpcErrorResponse {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.error });
        if let Some(message) = self.message {
            body["message"] = json!(message);
        }
        (self.status, Json(body)).into_response()
    }
}

/// Axum-compatible typed XRPC error wrapper.
///
/// Implements [`IntoResponse`] for generated endpoint error enums and common
/// XRPC client-side error variants.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("XRPC error: {error}")]
pub struct XrpcErrorResponse<E>
where
    E: std::error::Error,
{
    pub status: StatusCode,
    #[diagnostic_source]
    pub error: XrpcError<E>,
}

impl<E> XrpcErrorResponse<E>
where
    E: std::error::Error,
{
    /// Creates a new `XrpcErrorResponse` from the given status code and error.
    pub fn new(status: StatusCode, error: XrpcError<E>) -> Self {
        Self { status, error }
    }

    /// Changes the status code of the error response.
    pub fn with_status(self, status: StatusCode) -> Self {
        Self {
            status,
            error: self.error,
        }
    }
}

impl<E> IntoResponse for XrpcErrorResponse<E>
where
    E: std::error::Error + serde::Serialize,
{
    fn into_response(self) -> Response {
        let json = match self.error {
            XrpcError::Xrpc(error) => typed_error_json(&error),
            XrpcError::Auth(auth_error) => json!({
                "error": "Authentication",
                "message": format!("{auth_error}")
            }),
            XrpcError::Generic(generic) => typed_error_json(&generic),
            XrpcError::Decode(error) => json!({
                "error": "InvalidRequest",
                "message": format!("failed to decode request: {error}")
            }),
            _ => json!({
                "error": "InternalServerError",
                "message": "unknown error"
            }),
        };
        (self.status, Json(json)).into_response()
    }
}

impl<E> From<XrpcError<E>> for XrpcErrorResponse<E>
where
    E: std::error::Error,
{
    fn from(value: XrpcError<E>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: value,
        }
    }
}

impl<E> From<XrpcErrorResponse<E>> for XrpcError<E>
where
    E: std::error::Error,
{
    fn from(value: XrpcErrorResponse<E>) -> Self {
        value.error
    }
}

fn invalid_request(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "InvalidRequest",
            "message": message.into()
        })),
    )
        .into_response()
}

fn internal_server_error_response(message: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": "InternalServerError",
            "message": message.into()
        })),
    )
        .into_response()
}

fn typed_error_json(error: &(impl std::error::Error + serde::Serialize)) -> Value {
    serde_json::to_value(error).unwrap_or_else(|_| {
        json!({
            "error": "InternalServerError",
            "message": format!("{error}")
        })
    })
}
