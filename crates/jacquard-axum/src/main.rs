use axum::{Router, response::IntoResponse};
use jacquard::{
    IntoStatic,
    api::com_atproto::identity::resolve_did::{ResolveDid, ResolveDidOutput, ResolveDidRequest},
    identity::resolver::IdentityResolver,
    types::value::Data,
    xrpc::XrpcEndpoint,
};
use jacquard_axum::ExtractXrpc;
use jacquard_common::xrpc::XrpcMethod;
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::EnvFilter;

trait IntoRouter {
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

#[axum_macros::debug_handler]
async fn handler(ExtractXrpc(args): ExtractXrpc<ResolveDidRequest>) -> &'static str {
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
