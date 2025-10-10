use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jacquard::{
    IntoStatic,
    xrpc::{XrpcEndpoint, XrpcMethod, XrpcRequest},
};
use serde_json::json;

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
                            serde_json::to_string(&json!({
                                "error": "InvalidRequest",
                                "message": format!("failed to decode request: {}", err)
                            }))
                            .expect("Failed to serialize error response"),
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
                                    serde_json::to_string(&json!({
                                        "error": "InvalidRequest",
                                        "message": format!("failed to decode request: {}", e)
                                    }))
                                    .expect("Failed to serialize error response"),
                                )
                                    .into_response()
                            })?;
                        Ok(ExtractXrpc(value.into_static()))
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
