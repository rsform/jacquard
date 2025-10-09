use axum::{
    Router,
    body::{Body, Bytes},
    extract::{FromRequest, Request},
    http::{
        StatusCode,
        header::{HeaderValue, USER_AGENT},
        uri::PathAndQuery,
    },
    response::{ErrorResponse, IntoResponse, Response},
    routing::get,
};
use jacquard::types::xrpc::{XrpcError, XrpcMethod, XrpcRequest};
use serde::Serialize;
use serde_json::json;

pub struct ExtractXrpc<R: XrpcRequest<'static>>(Option<R>);

impl<S, R> FromRequest<S> for ExtractXrpc<R>
where
    Bytes: FromRequest<S>,
    S: Send + Sync,
    R: for<'de> XrpcRequest<'de> + for<'de> serde::Deserialize<'de>,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match R::METHOD {
            XrpcMethod::Procedure(encoding) => {
                let body = Bytes::from_request(req, state)
                    .await
                    .map_err(IntoResponse::into_response)?;
                match encoding {
                    "application/json" => {
                        let value: R = serde_json::from_slice::<R>(&body).map_err(|e| {
                            (
                                StatusCode::BAD_REQUEST,
                                serde_json::to_string(&json!({
                                    "error": "InvalidRequest",
                                    "message": XrpcRequestError::JsonDecodeError(e).to_string()
                                }))
                                .expect("Failed to serialize error response"),
                            )
                                .into_response()
                        })?;
                        Ok(ExtractXrpc(Some(value)))
                    }
                    "*/*" => {
                        let decoded = R::decode_body(&body);
                        match decoded {
                            Ok(value) => Ok(ExtractXrpc(Some(*value))),
                            Err(err) => Err((
                                StatusCode::BAD_REQUEST,
                                serde_json::to_string(&json!({
                                    "error": "InvalidRequest",
                                    "message": format!("failed to decode request: {}", err)
                                }))
                                .expect("Failed to serialize error response"),
                            )
                                .into_response()),
                        }
                    }
                    _ => todo!("handle other encodings"),
                }
            }
            XrpcMethod::Query => {
                if let Some(path_query) = req.uri().path_and_query() {
                    let path = path_query.path();
                    if path.ends_with(R::NSID) {
                        if let Some(query) = path_query.query() {
                            let value: R = serde_html_form::from_str(query).map_err(|e| {
                                (
                                    StatusCode::BAD_REQUEST,
                                    serde_json::to_string(&json!({
                                        "error": "InvalidRequest",
                                        "message": format!("failed to decode request: {}", e)
                                    }))
                                    .expect("Failed to serialize error response"),
                                )
                                    .into_response()
                            })?;
                            Ok(ExtractXrpc(Some(value)))
                        } else {
                            Ok(ExtractXrpc(None))
                        }
                    } else {
                        Err((
                            StatusCode::BAD_REQUEST,
                            serde_json::to_string(&json!({
                                "error": "InvalidRequest",
                                "message": "wrong nsid for wherever this ended up"
                            }))
                            .expect("Failed to serialize error response"),
                        )
                            .into_response())
                    }
                } else {
                    Err((
                        StatusCode::BAD_REQUEST,
                        serde_json::to_string(&json!({
                            "error": "InvalidRequest",
                            "message": "wrong nsid for wherever this ended up"
                        }))
                        .expect("Failed to serialize error response"),
                    )
                        .into_response())
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum XrpcRequestError {
    #[error("Unsupported encoding: {0}")]
    UnsupportedEncoding(String),
    #[error("JSON decode error: {0}")]
    JsonDecodeError(serde_json::Error),
    #[error("UTF-8 decode error: {0}")]
    Utf8DecodeError(std::string::FromUtf8Error),
}
