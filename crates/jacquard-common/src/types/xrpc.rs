use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Debug};

use crate::IntoStatic;
use crate::types::value::Data;

/// Error type for encoding XRPC requests
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum EncodeError {
    /// Failed to serialize query parameters
    #[error("Failed to serialize query: {0}")]
    Query(
        #[from]
        #[source]
        serde_html_form::ser::Error,
    ),
    /// Failed to serialize JSON body
    #[error("Failed to serialize JSON: {0}")]
    Json(
        #[from]
        #[source]
        serde_json::Error,
    ),
    /// Other encoding error
    #[error("Encoding error: {0}")]
    Other(String),
}

/// XRPC method type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrpcMethod {
    /// Query (HTTP GET)
    Query,
    /// Procedure (HTTP POST)
    Procedure(&'static str),
}

impl XrpcMethod {
    /// Get the HTTP method string
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Query => "GET",
            Self::Procedure(_) => "POST",
        }
    }

    pub const fn body_encoding(&self) -> Option<&'static str> {
        match self {
            Self::Query => None,
            Self::Procedure(enc) => Some(enc),
        }
    }
}

/// Trait for XRPC request types (queries and procedures)
///
/// This trait provides metadata about XRPC endpoints including the NSID,
/// HTTP method, encoding types, and associated output types.
///
/// The trait is implemented on the request parameters/input type itself.
pub trait XrpcRequest: Serialize {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// XRPC method (query/GET or procedure/POST)
    const METHOD: XrpcMethod;

    /// Output encoding (MIME type)
    const OUTPUT_ENCODING: &'static str;

    /// Response output type
    type Output<'de>: Deserialize<'de> + IntoStatic;

    /// Error type for this request
    type Err<'de>: Error + Deserialize<'de> + IntoStatic;

    /// Encode the request body for procedures.
    ///
    /// Default implementation serializes to JSON. Override for non-JSON encodings.
    fn encode_body(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(serde_json::to_vec(self)?)
    }
}

/// Error type for XRPC endpoints that don't define any errors
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct GenericError<'a>(Data<'a>);

impl fmt::Display for GenericError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for GenericError<'_> {}

impl IntoStatic for GenericError<'_> {
    type Output = GenericError<'static>;
    fn into_static(self) -> Self::Output {
        GenericError(self.0.into_static())
    }
}
