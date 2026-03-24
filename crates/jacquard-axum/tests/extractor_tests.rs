use axum::{Json, Router, response::IntoResponse};
use axum_test::TestServer;
use jacquard_axum::{ExtractXrpc, IntoRouter};
use jacquard_common::bos::{BosStr, DefaultStr};
use jacquard_common::types::string::Did;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Mock XRPC endpoint for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
struct TestQueryRequest<S: BosStr = DefaultStr> {
    did: Did<S>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
struct TestQueryResponse<S: BosStr = DefaultStr> {
    did: Did<S>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    extra_data: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("test error")]
struct TestError;

impl jacquard::xrpc::XrpcResp for TestQueryResponse {
    const NSID: &'static str = "com.example.test.query";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = TestQueryResponse<S>;
    type Err = TestError;
}

impl<S: BosStr + serde::Serialize> jacquard::xrpc::XrpcRequest for TestQueryRequest<S> {
    const NSID: &'static str = "com.example.test.query";
    const METHOD: jacquard::xrpc::XrpcMethod = jacquard::xrpc::XrpcMethod::Query;
    type Response = TestQueryResponse;
}

impl jacquard::xrpc::XrpcEndpoint for TestQueryRequest {
    const PATH: &'static str = "/xrpc/com.example.test.query";
    const METHOD: jacquard::xrpc::XrpcMethod = jacquard::xrpc::XrpcMethod::Query;
    type Request<S: BosStr> = TestQueryRequest<S>;
    type Response = TestQueryResponse;
}

async fn test_handler(
    ExtractXrpc(req): ExtractXrpc<TestQueryRequest, DefaultStr>,
) -> impl IntoResponse {
    Json(TestQueryResponse {
        did: req.did,
        extra_data: BTreeMap::new(),
    })
}

#[tokio::test]
async fn test_url_encoded_did_in_query_params() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_handler));

    let server = TestServer::new(app).unwrap();

    // Test with URL-encoded DID (colons should be encoded as %3A)
    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aplc%3A123abc")
        .await;

    response.assert_status_ok();

    let body_text = response.text();
    println!("URL-encoded test response: {}", body_text);
    let body: TestQueryResponse = serde_json::from_str(&body_text).unwrap();
    println!("Parsed DID: {}", body.did.as_str());
    assert_eq!(body.did.as_str(), "did:plc:123abc");
}

#[tokio::test]
async fn test_unencoded_did_in_query_params() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_handler));

    let server = TestServer::new(app).unwrap();

    // Test with unencoded DID (some clients might send it unencoded)
    let response = server
        .get("/xrpc/com.example.test.query?did=did:plc:123abc")
        .await;

    response.assert_status_ok();

    let body_text = response.text();
    println!("Unencoded test response: {}", body_text);
    let body: TestQueryResponse = serde_json::from_str(&body_text).unwrap();
    println!("Parsed DID: {}", body.did.as_str());
    assert_eq!(body.did.as_str(), "did:plc:123abc");
}

#[tokio::test]
async fn test_multiple_params_with_encoded_did() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_handler));

    let server = TestServer::new(app).unwrap();

    // Test with multiple params including URL-encoded DID
    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aweb%3Aexample.com&limit=50")
        .await;

    response.assert_status_ok();

    let body_text = response.text();
    println!("Multiple params test response: {}", body_text);
    let body: TestQueryResponse = serde_json::from_str(&body_text).unwrap();
    println!("Parsed DID: {}", body.did.as_str());
    assert_eq!(body.did.as_str(), "did:web:example.com");
}
