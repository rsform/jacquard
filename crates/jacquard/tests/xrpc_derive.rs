use jacquard::{BosStr, DefaultStr, IntoStatic};
use jacquard_derive::XrpcRequest;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// Test output type — uses BOS type parameter so it can borrow from the response buffer or own
// the data via the default `DefaultStr = SmolStr` backend.
#[derive(Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
pub struct GetThingOutput<S: BosStr = DefaultStr> {
    pub result: S,
}

// Test basic query endpoint
#[derive(Serialize, Deserialize, XrpcRequest)]
#[xrpc(nsid = "com.example.getThing", method = Query, output = GetThingOutput)]
pub struct GetThing<S: BosStr = DefaultStr> {
    pub id: S,
}

// Test procedure endpoint
#[derive(Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
pub struct CreateThingOutput<S: BosStr = DefaultStr> {
    pub id: S,
}

#[derive(Serialize, Deserialize, XrpcRequest)]
#[xrpc(
    nsid = "com.example.createThing",
    method = Procedure,
    output = CreateThingOutput
)]
pub struct CreateThing<S: BosStr = DefaultStr> {
    pub name: S,
}

// Test with custom error type — errors must be DeserializeOwned (no borrowing), so they use SmolStr
// directly rather than a BOS type parameter.
#[derive(Serialize, Deserialize, Debug, thiserror::Error)]
#[error("Custom error: {message}")]
pub struct CustomError {
    pub message: SmolStr,
}

impl jacquard::IntoStatic for CustomError {
    type Output = CustomError;
    fn into_static(self) -> Self::Output {
        self
    }
}

#[derive(Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
pub struct DoThingOutput<S: BosStr = DefaultStr> {
    pub status: S,
}

#[derive(Serialize, Deserialize, XrpcRequest)]
#[xrpc(
    nsid = "com.example.doThing",
    method = Procedure,
    output = DoThingOutput,
    error = CustomError
)]
pub struct DoThing<S: BosStr = DefaultStr> {
    pub param: S,
}

// Test server-side endpoint generation
#[derive(Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
pub struct ServerThingOutput<S: BosStr = DefaultStr> {
    pub status: S,
}

#[derive(Serialize, Deserialize, IntoStatic, XrpcRequest)]
#[xrpc(
    nsid = "com.example.serverThing",
    method = Query,
    output = ServerThingOutput,
    server
)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
pub struct ServerThing<S: BosStr = DefaultStr> {
    pub query: S,
}

#[test]
fn test_generated_response_markers() {
    // Just verify the types exist and compile
    let _: GetThingResponse;
    let _: CreateThingResponse;
    let _: DoThingResponse;
    let _: ServerThingResponse;
}

#[test]
fn test_xrpc_request_impl() {
    use jacquard::xrpc::{XrpcMethod, XrpcRequest};

    // Query endpoint
    assert_eq!(GetThing::<&str>::NSID, "com.example.getThing");
    assert!(matches!(GetThing::<&str>::METHOD, XrpcMethod::Query));

    // Procedure endpoint
    assert_eq!(CreateThing::<&str>::NSID, "com.example.createThing");
    assert!(matches!(
        CreateThing::<&str>::METHOD,
        XrpcMethod::Procedure("application/json")
    ));
}

#[test]
fn test_xrpc_endpoint_impl() {
    use jacquard::xrpc::XrpcEndpoint;

    // Server-side endpoint
    assert_eq!(ServerThingEndpoint::PATH, "/xrpc/com.example.serverThing");
    assert!(matches!(
        ServerThingEndpoint::METHOD,
        jacquard::xrpc::XrpcMethod::Query
    ));
}
