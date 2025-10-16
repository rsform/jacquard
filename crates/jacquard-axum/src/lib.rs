//! # Axum helpers for jacquard XRPC server implementations
//!
//! ## Usage
//!
//! ```no_run
//! use axum::{Router, routing::get, http::StatusCode, response::IntoResponse,  Json};
//! use jacquard_axum::{ ExtractXrpc, IntoRouter };
//! use std::collections::BTreeMap;
//! use miette::{IntoDiagnostic, Result};
//! use jacquard::api::com_atproto::identity::resolve_handle::{ResolveHandle, ResolveHandleRequest, ResolveHandleOutput};
//! use jacquard_common::types::string::Did;
//!
//! async fn handle_resolve(
//!     ExtractXrpc(req): ExtractXrpc<ResolveHandleRequest>
//! ) -> Result<Json<ResolveHandleOutput<'static>>, StatusCode> {
//!     // req is ResolveHandle<'static>, ready to use
//!     let handle = req.handle;
//!     // ... resolve logic
//! #   let output = ResolveHandleOutput { did: Did::new_static("did:plc:test").unwrap(), extra_data: BTreeMap::new()  };
//!     Ok(Json(output))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let app = Router::new()
//!          .route("/", axum::routing::get(|| async { "hello world!" }))
//!          .merge(ResolveHandleRequest::into_router(handle_resolve));
//!
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
//!         .await
//!         .into_diagnostic()?;
//!         axum::serve(listener, app).await.unwrap();
//!     Ok(())
//! }
//! ```
//!
//!
//! The extractor uses the [`XrpcEndpoint`] trait to determine request type:
//! - **Query**: Deserializes from query string parameters
//! - **Procedure**: Deserializes from request body (supports custom encodings via `decode_body`)
//!
//! Deserialization errors return a 400 Bad Request with a JSON error body matching
//! the XRPC error format.
//!
//! The extractor deserializes to borrowed types first, then converts to `'static` via
//! [`IntoStatic`], avoiding the DeserializeOwned requirement of the Json axum extractor and similar.

pub mod did_web;
#[cfg(feature = "service-auth")]
pub mod service_auth;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jacquard::{
    IntoStatic,
    xrpc::{XrpcEndpoint, XrpcError, XrpcMethod, XrpcRequest},
};
use serde_json::json;

/// Axum extractor for XRPC requests
///
/// Deserializes incoming requests based on the endpoint's method type (Query or Procedure)
/// and returns the owned (`'static`) request type ready for handler logic.

pub struct ExtractXrpc<E: XrpcEndpoint>(pub E::Request<'static>);

impl<S, R> FromRequest<S> for ExtractXrpc<R>
where
    S: Send + Sync,
    R: XrpcEndpoint,
    for<'a> R::Request<'a>: IntoStatic<Output = R::Request<'static>>,
{
    type Rejection = Response;

    fn from_request(
        req: Request,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async {
            match R::METHOD {
                XrpcMethod::Procedure(_) => {
                    let body = Bytes::from_request(req, state)
                        .await
                        .map_err(IntoResponse::into_response)?;
                    let decoded = R::Request::decode_body(&body);
                    match decoded {
                        Ok(value) => Ok(ExtractXrpc(*value.into_static())),
                        Err(err) => Err((
                            StatusCode::BAD_REQUEST,
                            Json(json!({
                                "error": "InvalidRequest",
                                "message": format!("failed to decode request: {}", err)
                            })),
                        )
                            .into_response()),
                    }
                }
                XrpcMethod::Query => {
                    if let Some(path_query) = req.uri().path_and_query() {
                        let query = path_query.query().unwrap_or("");
                        let value: R::Request<'_> =
                            serde_html_form::from_str::<R::Request<'_>>(query).map_err(|e| {
                                (
                                    StatusCode::BAD_REQUEST,
                                    Json(json!({
                                        "error": "InvalidRequest",
                                        "message": format!("failed to decode request: {}", e)
                                    })),
                                )
                                    .into_response()
                            })?;
                        Ok(ExtractXrpc(value.into_static()))
                    } else {
                        Err((
                            StatusCode::BAD_REQUEST,
                            Json(json!({
                                "error": "InvalidRequest",
                                "message": "wrong path"
                            })),
                        )
                            .into_response())
                    }
                }
            }
        }
    }
}

/// Conversion trait to turn an XrpcEndpoint and a handler into an axum Router
pub trait IntoRouter {
    fn into_router<T, S, U>(handler: U) -> Router<S>
    where
        T: 'static,
        S: Clone + Send + Sync + 'static,
        U: axum::handler::Handler<T, S>;
}

impl<X> IntoRouter for X
where
    X: XrpcEndpoint,
{
    /// Creates an axum router that will invoke `handler` in response to xrpc
    /// request `X`.
    fn into_router<T, S, U>(handler: U) -> Router<S>
    where
        T: 'static,
        S: Clone + Send + Sync + 'static,
        U: axum::handler::Handler<T, S>,
    {
        Router::new().route(
            X::PATH,
            (match X::METHOD {
                XrpcMethod::Query => axum::routing::get,
                XrpcMethod::Procedure(_) => axum::routing::post,
            })(handler),
        )
    }
}

/// Axum-compatible Xrpc error wrapper
///
/// Implements IntoResponse, and does some mildly opinionated mapping.
///
/// Currently assumes that the internal xrpc errors are well-formed and
/// compatible with [the spec](https://atproto.com/specs/xrpc#error-responses).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("Xrpc error: {error}")]
pub struct XrpcErrorResponse<E>
where
    E: std::error::Error + IntoStatic,
{
    pub status: StatusCode,
    #[diagnostic_source]
    pub error: XrpcError<E>,
}

impl<E> XrpcErrorResponse<E>
where
    E: std::error::Error + IntoStatic + serde::Serialize,
{
    /// Creates a new XrpcErrorResponse from the given status code and error.
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
    E: std::error::Error + IntoStatic + serde::Serialize,
{
    fn into_response(self) -> Response {
        let (status, json) = match self.error {
            XrpcError::Xrpc(error) => (
                self.status,
                serde_json::to_value(&error).unwrap_or(json!({
                    "error": "InternalError",
                    "message": format!("{error}")
                })),
            ),
            XrpcError::Auth(auth_error) => (
                self.status,
                json!({
                    "error": "Authentication",
                    "message": format!("{auth_error}")
                }),
            ),
            XrpcError::Generic(generic) => (
                self.status,
                serde_json::to_value(&generic).unwrap_or(json!({
                    "error": "InternalError",
                    "message": format!("{generic}", )
                })),
            ),
            XrpcError::Decode(error) => (
                self.status,
                json!({
                    "error": "InvalidRequest",
                    "message": format!("failed to decode request: {error}", )
                }),
            ),
        };
        (status, Json(json)).into_response()
    }
}

impl<E> From<XrpcError<E>> for XrpcErrorResponse<E>
where
    E: std::error::Error + IntoStatic,
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
    E: std::error::Error + IntoStatic,
{
    fn from(value: XrpcErrorResponse<E>) -> Self {
        value.error
    }
}
