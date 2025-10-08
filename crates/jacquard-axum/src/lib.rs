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

pub struct ExtractXrpc<R: XrpcRequest<'static>>(R);

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
                        Ok(ExtractXrpc(value))
                    }
                    "*/*" => {
                        //
                        todo!()
                    }
                    _ => todo!(),
                }
            }
            XrpcMethod::Query => {
                if let Some(path_query) = req.uri().path_and_query() {
                    let path = path_query.path();
                    let query = path_query.query();
                    if path.ends_with(R::NSID) {
                        // success
                    } else {
                        // wrong endpoint
                    }
                }

                todo!()
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
