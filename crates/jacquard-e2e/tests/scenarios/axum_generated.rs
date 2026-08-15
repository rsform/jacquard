//! Native generated-XRPC server contracts. This target is feature-gated with
//! the full-stack harness but needs no provider or container.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use jacquard_axum::{ExtractXrpc, IntoRouter, XrpcErrorResponse, XrpcResponse};
use jacquard_codegen_tests::pretty::test_blobby::get::{GetError, GetOutput, GetRequest};
use jacquard_codegen_tests::pretty::test_blobby::put::{PutOutput, PutRequest};
use jacquard_codegen_tests::pretty::test_blobby::record::Record;
use jacquard_common::DefaultStr;
use jacquard_common::xrpc::XrpcError;
use tower::ServiceExt;

const PNG_CID: &str = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";
const URI: &str = "at://did:example:alice/test.blobby.record/native";

fn record() -> Record {
    Record {
        primary: serde_json::from_value(serde_json::json!({
            "$type": "blob",
            "ref": { "$link": PNG_CID },
            "mimeType": "image/png",
            "size": 32
        }))
        .expect("fixture blob is valid"),
        secondary: None,
        note: Some(DefaultStr::new("native axum")),
        extra_data: None,
    }
}

async fn get_handler(ExtractXrpc(request): ExtractXrpc<GetRequest>) -> XrpcResponse<GetRequest> {
    XrpcResponse(GetOutput {
        uri: request.uri,
        value: record(),
        extra_data: None,
    })
}

async fn put_handler(ExtractXrpc(request): ExtractXrpc<PutRequest>) -> XrpcResponse<PutRequest> {
    XrpcResponse(PutOutput {
        cid: None,
        uri: request.uri,
        extra_data: None,
    })
}

async fn declared_error_handler(
    ExtractXrpc(_): ExtractXrpc<GetRequest>,
) -> Result<XrpcResponse<GetRequest>, XrpcErrorResponse<GetError>> {
    Err(XrpcErrorResponse::new(
        StatusCode::NOT_FOUND,
        XrpcError::Xrpc(GetError::BlobbyNotFound(Some(DefaultStr::new(
            "fixture record is absent",
        )))),
    ))
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("read response body");
    serde_json::from_slice(&bytes).expect("response is JSON")
}

#[tokio::test]
async fn axum_generated_query_contract() {
    let app = GetRequest::into_router(get_handler);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/xrpc/test.blobby.get?uri={}",
                    URI.replace(':', "%3A").replace('/', "%2F")
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body = body_json(response).await;
    assert_eq!(body["uri"], URI);
    assert_eq!(body["value"]["note"], "native axum");
    assert_eq!(body["value"]["primary"]["ref"]["$link"], PNG_CID);
}

#[tokio::test]
async fn axum_generated_procedure_contract() {
    let app = PutRequest::into_router(put_handler);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/xrpc/test.blobby.put")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "uri": URI,
                        "value": record()
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body, serde_json::json!({ "uri": URI }));
}

#[tokio::test]
async fn axum_declared_error_contract() {
    let app = GetRequest::into_router(declared_error_handler);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/xrpc/test.blobby.get?uri={}",
                    URI.replace(':', "%3A").replace('/', "%2F")
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({
            "error": "BlobbyNotFound",
            "message": "fixture record is absent"
        })
    );
}

#[tokio::test]
async fn axum_rejects_malformed_input() {
    let calls = Arc::new(AtomicUsize::new(0));
    let query_calls = calls.clone();
    let query_app = GetRequest::into_router(move |request: ExtractXrpc<GetRequest>| {
        let calls = query_calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            get_handler(request).await
        }
    });
    let query = query_app
        .oneshot(
            Request::builder()
                .uri("/xrpc/test.blobby.get?uri=not-an-at-uri")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(query.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(query).await["error"], "InvalidRequest");

    let procedure_calls = calls.clone();
    let procedure_app = PutRequest::into_router(move |request: ExtractXrpc<PutRequest>| {
        let calls = procedure_calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            put_handler(request).await
        }
    });
    let procedure = procedure_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/xrpc/test.blobby.put")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(br#"{"uri":42}"#.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(procedure.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(procedure).await["error"], "InvalidRequest");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "handlers were not invoked");
}
