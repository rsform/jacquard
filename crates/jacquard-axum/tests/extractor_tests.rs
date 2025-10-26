use axum::{Json, Router, response::IntoResponse};
use axum_test::TestServer;
use jacquard_axum::{ExtractXrpc, IntoRouter};
use jacquard_common::types::string::Did;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Mock XRPC endpoint for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestQueryRequest<'a> {
    #[serde(borrow)]
    did: Did<'a>,
    #[serde(default)]
    limit: Option<u32>,
}

impl jacquard::IntoStatic for TestQueryRequest<'_> {
    type Output = TestQueryRequest<'static>;

    fn into_static(self) -> Self::Output {
        TestQueryRequest {
            did: self.did.into_static(),
            limit: self.limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestQueryResponse<'a> {
    #[serde(borrow)]
    did: Did<'a>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    extra_data: BTreeMap<String, serde_json::Value>,
}

impl jacquard::IntoStatic for TestQueryResponse<'_> {
    type Output = TestQueryResponse<'static>;

    fn into_static(self) -> Self::Output {
        TestQueryResponse {
            did: self.did.into_static(),
            extra_data: self.extra_data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("test error")]
struct TestError;

impl jacquard::IntoStatic for TestError {
    type Output = TestError;

    fn into_static(self) -> Self::Output {
        self
    }
}

impl jacquard::xrpc::XrpcResp for TestQueryResponse<'_> {
    const NSID: &'static str = "com.example.test.query";
    const ENCODING: &'static str = "application/json";
    type Output<'a> = TestQueryResponse<'a>;
    type Err<'a> = TestError;
}

impl jacquard::xrpc::XrpcRequest for TestQueryRequest<'_> {
    const NSID: &'static str = "com.example.test.query";
    const METHOD: jacquard::xrpc::XrpcMethod = jacquard::xrpc::XrpcMethod::Query;
    type Response = TestQueryResponse<'static>;
}

impl jacquard::xrpc::XrpcEndpoint for TestQueryRequest<'_> {
    const PATH: &'static str = "/xrpc/com.example.test.query";
    const METHOD: jacquard::xrpc::XrpcMethod = jacquard::xrpc::XrpcMethod::Query;
    type Request<'a> = TestQueryRequest<'a>;
    type Response = TestQueryResponse<'static>;
}

async fn test_handler(ExtractXrpc(req): ExtractXrpc<TestQueryRequest<'_>>) -> impl IntoResponse {
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
