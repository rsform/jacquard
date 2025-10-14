use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use jacquard::{
    api::com_atproto::identity::resolve_did::{ResolveDidOutput, ResolveDidRequest},
    identity::{JacquardResolver, resolver::IdentityResolver},
    types::value::to_data,
};
use jacquard_axum::{ExtractXrpc, IntoRouter};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::EnvFilter;

#[axum_macros::debug_handler]
async fn resolve_did(
    State(state): State<Arc<AppState>>,
    ExtractXrpc(args): ExtractXrpc<ResolveDidRequest>,
) -> Result<Json<ResolveDidOutput<'static>>, XrpcErrorResponse> {
    let doc = state
        .resolver
        .resolve_did_doc_owned(&args.did)
        .await
        .map_err(|_| XrpcErrorResponse::internal_server_error())?;
    Ok(ResolveDidOutput {
        did_doc: to_data(&doc).map_err(|_| XrpcErrorResponse::internal_server_error())?,
        extra_data: Default::default(),
    }
    .into())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_env_filter(EnvFilter::from_env("QDPDS_LOG"))
        .init();
    let app = Router::new()
        .route("/", axum::routing::get(|| async { "hello world!" }))
        .merge(ResolveDidRequest::into_router(resolve_did))
        .with_state(Arc::new(AppState::new()))
        .layer(tower_http::trace::TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .into_diagnostic()?;
    axum::serve(listener, app).await.into_diagnostic()?;
    Ok(())
}

pub struct XrpcErrorResponse {
    error: XrpcError,
    pub status: StatusCode,
}

impl XrpcErrorResponse {
    pub fn internal_server_error() -> Self {
        Self {
            error: XrpcError {
                error: "InternalServerError".to_string(),
                message: None,
            },
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct XrpcError {
    pub error: String,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub message: Option<String>,
}

impl IntoResponse for XrpcErrorResponse {
    fn into_response(self) -> axum::response::Response {
        Json(self.error).into_response()
    }
}

pub struct AppState {
    pub resolver: JacquardResolver,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: jacquard::identity::slingshot_resolver_default(),
        }
    }
}
