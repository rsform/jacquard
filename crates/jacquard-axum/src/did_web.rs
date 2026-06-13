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
use jacquard::deps::smol_str::SmolStr;
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
/// # async fn example(did_doc: DidDocument) {
/// let app = Router::new()
///     .merge(did_web_router(did_doc));
/// # }
/// ```
pub fn did_web_router(did_doc: DidDocument<SmolStr>) -> Router {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use jacquard::deps::smol_str::SmolStr;
    use jacquard::types::string::Did;
    use jacquard_common::types::did_doc::DidDocument;
    use tower::ServiceExt;

    // A minimal but spec-shaped DID document used by the did:web router tests.
    fn sample_did_document() -> DidDocument<SmolStr> {
        DidDocument {
            context: vec![SmolStr::new_static("https://www.w3.org/ns/did/v1")],
            id: Did::new_static("did:web:example.com").unwrap(),
            also_known_as: None,
            verification_method: None,
            service: Some(vec![]),
            extra_data: std::collections::BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn did_web_router_serves_document_at_well_known_path() {
        let doc = sample_did_document();
        let expected = serde_json::to_value(&doc).unwrap();
        let app = did_web_router(doc);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/.well-known/did.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/did+json"
        );
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body, expected);
    }

    #[tokio::test]
    async fn did_web_router_rejects_unknown_paths() {
        let app = did_web_router(sample_did_document());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/elsewhere")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
