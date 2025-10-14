//! Helper for serving did:web DID documents
//!
//! did:web DIDs resolve to HTTPS endpoints serving DID documents. This module
//! provides a router that serves your service's DID document at `/.well-known/did.json`.
//!
//! # Example
//!
//! ```no_run
//! use axum::Router;
//! use jacquard_axum::did_web::did_web_router;
//! use jacquard_common::types::did_doc::DidDocument;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Your DID document (typically loaded from config or generated)
//!     let did_doc: DidDocument = serde_json::from_str(r#"{
//!         "id": "did:web:feedgen.example.com",
//!         "verificationMethod": [{
//!             "id": "did:web:feedgen.example.com#atproto",
//!             "type": "Multikey",
//!             "controller": "did:web:feedgen.example.com",
//!             "publicKeyMultibase": "zQ3sh..."
//!         }]
//!     }"#).unwrap();
//!
//!     let app = Router::new()
//!         .merge(did_web_router(did_doc));
//!
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:443")
//!         .await
//!         .unwrap();
//!     axum::serve(listener, app).await.unwrap();
//! }
//! ```

use axum::{
    Json, Router,
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use jacquard_common::types::did_doc::DidDocument;

/// Create a router that serves a DID document at `/.well-known/did.json`
///
/// Returns a Router that can be merged into your main application router.
/// The DID document is cloned on each request.
///
/// # Example
///
/// ```no_run
/// use axum::Router;
/// use jacquard_axum::did_web::did_web_router;
/// use jacquard_common::types::did_doc::DidDocument;
///
/// # async fn example(did_doc: DidDocument<'static>) {
/// let app = Router::new()
///     .merge(did_web_router(did_doc));
/// # }
/// ```
pub fn did_web_router(did_doc: DidDocument<'static>) -> Router {
    Router::new().route(
        "/.well-known/did.json",
        get(move || async move {
            (
                StatusCode::OK,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/did+json"),
                )],
                Json(did_doc.clone()),
            )
                .into_response()
        }),
    )
}
