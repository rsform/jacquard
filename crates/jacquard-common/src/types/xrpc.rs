use serde::{Deserialize, Serialize};
use std::error::Error;

/// XRPC method type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrpcMethod {
    /// Query (HTTP GET)
    Query,
    /// Procedure (HTTP POST)
    Procedure,
}

impl XrpcMethod {
    /// Get the HTTP method string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Query => "GET",
            Self::Procedure => "POST",
        }
    }
}

/// Trait for XRPC request types (queries and procedures)
///
/// This trait provides metadata about XRPC endpoints including the NSID,
/// HTTP method, encoding types, and associated parameter/output types.
pub trait XrpcRequest {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;

    /// Input encoding (MIME type, e.g., "application/json")
    /// None for queries (no body)
    const INPUT_ENCODING: Option<&'static str>;

    /// Output encoding (MIME type)
    const OUTPUT_ENCODING: &'static str;

    /// Request parameters type (query params for queries, body for procedures)
    type Params: Serialize;

    /// Response output type
    type Output: for<'de> Deserialize<'de>;

    /// Error type for this request
    type Err: Error;
}
