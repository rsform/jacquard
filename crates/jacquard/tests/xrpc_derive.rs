use jacquard::{CowStr, IntoStatic};
use jacquard_derive::XrpcRequest;
use serde::{Deserialize, Serialize};

// Test output type
#[derive(Serialize, Deserialize, IntoStatic)]
pub struct GetThingOutput<'a> {
    #[serde(borrow)]
    pub result: CowStr<'a>,
}

// Test basic query endpoint
#[derive(Serialize, Deserialize, XrpcRequest)]
#[xrpc(nsid = "com.example.getThing", method = Query, output = GetThingOutput)]
pub struct GetThing<'a> {
    #[serde(borrow)]
    pub id: CowStr<'a>,
}

// Test procedure endpoint
#[derive(Serialize, Deserialize, IntoStatic)]
pub struct CreateThingOutput<'a> {
    #[serde(borrow)]
    pub id: CowStr<'a>,
}

#[derive(Serialize, Deserialize, XrpcRequest)]
#[xrpc(
    nsid = "com.example.createThing",
    method = Procedure,
    output = CreateThingOutput
)]
pub struct CreateThing<'a> {
    #[serde(borrow)]
    pub name: CowStr<'a>,
}

// Test with custom error type
#[derive(Serialize, Deserialize, Debug, thiserror::Error)]
#[error("Custom error")]
pub struct CustomError<'a> {
    #[serde(borrow)]
    pub message: CowStr<'a>,
}

impl jacquard::IntoStatic for CustomError<'_> {
    type Output = CustomError<'static>;
    fn into_static(self) -> Self::Output {
        CustomError {
            message: self.message.into_static(),
        }
    }
}

#[derive(Serialize, Deserialize, IntoStatic)]
pub struct DoThingOutput<'a> {
    #[serde(borrow)]
    pub status: CowStr<'a>,
}

#[derive(Serialize, Deserialize, XrpcRequest)]
#[xrpc(
    nsid = "com.example.doThing",
    method = Procedure,
    output = DoThingOutput,
    error = CustomError
)]
pub struct DoThing<'a> {
    #[serde(borrow)]
    pub param: CowStr<'a>,
}

// Test server-side endpoint generation
#[derive(Serialize, Deserialize, IntoStatic)]
pub struct ServerThingOutput<'a> {
    #[serde(borrow)]
    pub status: CowStr<'a>,
}

#[derive(Serialize, Deserialize, IntoStatic, XrpcRequest)]
#[xrpc(
    nsid = "com.example.serverThing",
    method = Query,
    output = ServerThingOutput,
    server
)]
pub struct ServerThing<'a> {
    #[serde(borrow)]
    pub query: CowStr<'a>,
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
    assert_eq!(GetThing::NSID, "com.example.getThing");
    assert!(matches!(GetThing::METHOD, XrpcMethod::Query));

    // Procedure endpoint
    assert_eq!(CreateThing::NSID, "com.example.createThing");
    assert!(matches!(
        CreateThing::METHOD,
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
