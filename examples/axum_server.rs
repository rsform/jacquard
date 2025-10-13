use axum::Router;
use jacquard::api::com_atproto::identity::resolve_did::ResolveDidRequest;
use jacquard_axum::{ExtractXrpc, IntoRouter};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::EnvFilter;

#[axum_macros::debug_handler]
async fn handler(ExtractXrpc(_args): ExtractXrpc<ResolveDidRequest>) -> &'static str {
    "hello world!"
    // let res = jacquard::identity::slingshot_resolver_default();
    // let doc = res.resolve_did_doc(&args.did).await?;
    // let valid_doc = doc.parse()?;
    // let doc_value = serde_json::to_value(valid_doc).unwrap();
    // Ok(ResolveDidOutput {
    //     did_doc: Data::from_json(&doc_value).unwrap().into_static(),
    //     extra_data: Default::default(),
    // }
    // .into())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_env_filter(EnvFilter::from_env("QDPDS_LOG"))
        .init();
    let app = Router::new()
        .route("/", axum::routing::get(|| async { "hello world!" }))
        .merge(ResolveDidRequest::into_router(handler))
        .layer(tower_http::trace::TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .into_diagnostic()?;
    axum::serve(listener, app).await.unwrap();
    Ok(())
}
