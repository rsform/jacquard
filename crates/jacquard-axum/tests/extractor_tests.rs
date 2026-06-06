use axum::{Json, Router, http::StatusCode, response::IntoResponse};
use axum_test::TestServer;
use jacquard::xrpc::{XrpcEndpoint, XrpcMethod, XrpcRequest, XrpcResp};
use jacquard_axum::{ExtractXrpc, IntoRouter, XrpcResponse};
use jacquard_common::bos::{BosStr, DefaultStr};
use jacquard_common::types::string::Did;
use jacquard_derive::IntoStatic;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap};

#[derive(Debug, Clone, Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
struct TestQueryInput<S: BosStr = DefaultStr> {
    did: Did<S>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
struct TestQueryOutput<S: BosStr = DefaultStr> {
    did: Did<S>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    extra_data: BTreeMap<String, serde_json::Value>,
}

struct TestQueryResponse;
struct TestQueryRequest;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("test error")]
struct TestError;

impl XrpcResp for TestQueryResponse {
    const NSID: &'static str = "com.example.test.query";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = TestQueryOutput<S>;
    type Err = TestError;
}

impl<S: BosStr> XrpcRequest for TestQueryInput<S> {
    const NSID: &'static str = "com.example.test.query";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = TestQueryResponse;
}

impl XrpcEndpoint for TestQueryRequest {
    const PATH: &'static str = "/xrpc/com.example.test.query";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Request<S: BosStr> = TestQueryInput<S>;
    type Response = TestQueryResponse;
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
struct TestProcedureInput<S: BosStr = DefaultStr> {
    did: Did<S>,
    active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
struct TestProcedureOutput<S: BosStr = DefaultStr> {
    did: Did<S>,
    active: bool,
}

struct TestProcedureResponse;
struct TestProcedureRequest;

impl XrpcResp for TestProcedureResponse {
    const NSID: &'static str = "com.example.test.procedure";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = TestProcedureOutput<S>;
    type Err = TestError;
}

impl<S: BosStr> XrpcRequest for TestProcedureInput<S> {
    const NSID: &'static str = "com.example.test.procedure";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Response = TestProcedureResponse;
}

impl XrpcEndpoint for TestProcedureRequest {
    const PATH: &'static str = "/xrpc/com.example.test.procedure";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Request<S: BosStr> = TestProcedureInput<S>;
    type Response = TestProcedureResponse;
}

async fn test_query_handler(ExtractXrpc(req): ExtractXrpc<TestQueryRequest>) -> impl IntoResponse {
    Json(TestQueryOutput {
        did: req.did,
        extra_data: BTreeMap::new(),
    })
}

async fn typed_query_handler(
    ExtractXrpc(req): ExtractXrpc<TestQueryRequest>,
) -> XrpcResponse<TestQueryRequest> {
    XrpcResponse(TestQueryOutput {
        did: req.did,
        extra_data: BTreeMap::new(),
    })
}

async fn string_query_handler(
    ExtractXrpc(req): ExtractXrpc<TestQueryRequest, String>,
) -> impl IntoResponse {
    Json(TestQueryOutput {
        did: req.did,
        extra_data: BTreeMap::new(),
    })
}

async fn cowstr_query_handler(
    ExtractXrpc(req): ExtractXrpc<TestQueryRequest, jacquard::CowStr<'static>>,
) -> impl IntoResponse {
    Json(TestQueryOutput {
        did: req.did,
        extra_data: BTreeMap::new(),
    })
}

async fn std_cow_query_handler(
    ExtractXrpc(req): ExtractXrpc<TestQueryRequest, Cow<'static, str>>,
) -> impl IntoResponse {
    Json(TestQueryOutput {
        did: req.did,
        extra_data: BTreeMap::new(),
    })
}

async fn test_procedure_handler(
    ExtractXrpc(req): ExtractXrpc<TestProcedureRequest>,
) -> impl IntoResponse {
    Json(TestProcedureOutput {
        did: req.did,
        active: req.active,
    })
}

async fn string_procedure_handler(
    ExtractXrpc(req): ExtractXrpc<TestProcedureRequest, String>,
) -> impl IntoResponse {
    Json(TestProcedureOutput {
        did: req.did,
        active: req.active,
    })
}

async fn cowstr_procedure_handler(
    ExtractXrpc(req): ExtractXrpc<TestProcedureRequest, jacquard::CowStr<'static>>,
) -> impl IntoResponse {
    Json(TestProcedureOutput {
        did: req.did,
        active: req.active,
    })
}

async fn std_cow_procedure_handler(
    ExtractXrpc(req): ExtractXrpc<TestProcedureRequest, Cow<'static, str>>,
) -> impl IntoResponse {
    Json(TestProcedureOutput {
        did: req.did,
        active: req.active,
    })
}

#[tokio::test]
async fn test_url_encoded_did_in_query_params() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aplc%3A123abc")
        .await;

    response.assert_status_ok();

    let body_text = response.text();
    let body: TestQueryOutput = serde_json::from_str(&body_text).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:123abc");
}

#[tokio::test]
async fn test_unencoded_did_in_query_params() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did:plc:123abc")
        .await;

    response.assert_status_ok();

    let body_text = response.text();
    let body: TestQueryOutput = serde_json::from_str(&body_text).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:123abc");
}

#[tokio::test]
async fn test_multiple_params_with_encoded_did() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aweb%3Aexample.com&limit=50")
        .await;

    response.assert_status_ok();

    let body_text = response.text();
    let body: TestQueryOutput = serde_json::from_str(&body_text).unwrap();
    assert_eq!(body.did.as_str(), "did:web:example.com");
}

#[tokio::test]
async fn test_string_extractor_decodes_query() {
    let app = Router::new().merge(TestQueryRequest::into_router(string_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aplc%3Astring")
        .await;

    response.assert_status_ok();
    let body: TestQueryOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:string");
}

#[tokio::test]
async fn test_cowstr_static_extractor_decodes_query() {
    let app = Router::new().merge(TestQueryRequest::into_router(cowstr_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aplc%3Acowstr")
        .await;

    response.assert_status_ok();
    let body: TestQueryOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:cowstr");
}

#[tokio::test]
async fn test_std_cow_static_extractor_decodes_query() {
    let app = Router::new().merge(TestQueryRequest::into_router(std_cow_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aplc%3Astd-cow")
        .await;

    response.assert_status_ok();
    let body: TestQueryOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:std-cow");
}

#[tokio::test]
async fn test_malformed_query_returns_xrpc_invalid_request() {
    let app = Router::new().merge(TestQueryRequest::into_router(test_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server.get("/xrpc/com.example.test.query?limit=50").await;

    response.assert_status_bad_request();
    let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body["error"], "InvalidRequest");
}

#[tokio::test]
async fn test_procedure_post_decodes_body() {
    let app = Router::new().merge(TestProcedureRequest::into_router(test_procedure_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/xrpc/com.example.test.procedure")
        .json(&serde_json::json!({
            "did": "did:plc:procedure",
            "active": true
        }))
        .await;

    response.assert_status_ok();
    let body: TestProcedureOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:procedure");
    assert!(body.active);
}

#[tokio::test]
async fn test_string_procedure_extractor_decodes_body() {
    let app = Router::new().merge(TestProcedureRequest::into_router(string_procedure_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/xrpc/com.example.test.procedure")
        .json(&serde_json::json!({
            "did": "did:plc:string-procedure",
            "active": true
        }))
        .await;

    response.assert_status_ok();
    let body: TestProcedureOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:string-procedure");
    assert!(body.active);
}

#[tokio::test]
async fn test_cowstr_static_procedure_extractor_decodes_body() {
    let app = Router::new().merge(TestProcedureRequest::into_router(cowstr_procedure_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/xrpc/com.example.test.procedure")
        .json(&serde_json::json!({
            "did": "did:plc:cowstr-procedure",
            "active": true
        }))
        .await;

    response.assert_status_ok();
    let body: TestProcedureOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:cowstr-procedure");
    assert!(body.active);
}

#[tokio::test]
async fn test_std_cow_static_procedure_extractor_decodes_body() {
    let app = Router::new().merge(TestProcedureRequest::into_router(std_cow_procedure_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/xrpc/com.example.test.procedure")
        .json(&serde_json::json!({
            "did": "did:plc:std-cow-procedure",
            "active": true
        }))
        .await;

    response.assert_status_ok();
    let body: TestProcedureOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:std-cow-procedure");
    assert!(body.active);
}

#[tokio::test]
async fn test_get_to_procedure_route_is_rejected() {
    let app = Router::new().merge(TestProcedureRequest::into_router(test_procedure_handler));

    let server = TestServer::new(app).unwrap();

    let response = server.get("/xrpc/com.example.test.procedure").await;

    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_malformed_procedure_body_returns_xrpc_invalid_request() {
    let app = Router::new().merge(TestProcedureRequest::into_router(test_procedure_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/xrpc/com.example.test.procedure")
        .text("{not valid json")
        .await;

    response.assert_status_bad_request();
    let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body["error"], "InvalidRequest");
}

#[tokio::test]
async fn test_xrpc_response_encodes_typed_output() {
    let app = Router::new().merge(TestQueryRequest::into_router(typed_query_handler));

    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/xrpc/com.example.test.query?did=did%3Aplc%3Atyped")
        .await;

    response.assert_status_ok();
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.starts_with(TestQueryResponse::ENCODING));

    let body: TestQueryOutput = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body.did.as_str(), "did:plc:typed");
}
