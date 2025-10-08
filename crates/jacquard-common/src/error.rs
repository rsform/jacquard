//! Error types for XRPC client operations

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::types::xrpc::EncodeError;

/// Client error type wrapping all possible error conditions
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ClientError {
    /// HTTP transport error
    #[error("HTTP transport error: {0}")]
    Transport(
        #[from]
        #[diagnostic_source]
        TransportError,
    ),

    /// Request serialization failed
    #[error("{0}")]
    Encode(
        #[from]
        #[diagnostic_source]
        EncodeError,
    ),

    /// Response deserialization failed
    #[error("{0}")]
    Decode(
        #[from]
        #[diagnostic_source]
        DecodeError,
    ),

    /// HTTP error response
    #[error("HTTP {0}")]
    Http(
        #[from]
        #[diagnostic_source]
        HttpError,
    ),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Auth(
        #[from]
        #[diagnostic_source]
        AuthError,
    ),
}

/// Transport-level errors that occur during HTTP communication
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum TransportError {
    /// Failed to establish connection to server
    #[error("Connection error: {0}")]
    Connect(String),

    /// Request timed out
    #[error("Request timeout")]
    Timeout,

    /// Request construction failed (malformed URI, headers, etc.)
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Other transport error
    #[error("Transport error: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Response deserialization errors
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum DecodeError {
    /// JSON deserialization failed
    #[error("Failed to deserialize JSON: {0}")]
    Json(
        #[from]
        #[source]
        serde_json::Error,
    ),
    /// CBOR deserialization failed (local I/O)
    #[error("Failed to deserialize CBOR: {0}")]
    CborLocal(
        #[from]
        #[source]
        serde_ipld_dagcbor::DecodeError<std::io::Error>,
    ),
    /// CBOR deserialization failed (remote/reqwest)
    #[error("Failed to deserialize CBOR: {0}")]
    CborRemote(
        #[from]
        #[source]
        serde_ipld_dagcbor::DecodeError<HttpError>,
    ),
}

/// HTTP error response (non-200 status codes outside of XRPC error handling)
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub struct HttpError {
    /// HTTP status code
    pub status: http::StatusCode,
    /// Response body if available
    pub body: Option<Bytes>,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}", self.status)?;
        if let Some(body) = &self.body {
            if let Ok(s) = std::str::from_utf8(body) {
                write!(f, ":\n{}", s)?;
            }
        }
        Ok(())
    }
}

/// Result type for client operations
pub type XrpcResult<T> = std::result::Result<T, ClientError>;

#[cfg(feature = "reqwest-client")]
impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout
        } else if e.is_connect() {
            Self::Connect(e.to_string())
        } else if e.is_builder() || e.is_request() {
            Self::InvalidRequest(e.to_string())
        } else {
            Self::Other(Box::new(e))
        }
    }
}

/// Authentication and authorization errors
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum AuthError {
    /// Access token has expired (use refresh token to get a new one)
    #[error("Access token expired")]
    TokenExpired,

    /// Access token is invalid or malformed
    #[error("Invalid access token")]
    InvalidToken,

    /// Token refresh request failed
    #[error("Token refresh failed")]
    RefreshFailed,

    /// Request requires authentication but none was provided
    #[error("No authentication provided, but endpoint requires auth")]
    NotAuthenticated,

    /// Other authentication error
    #[error("Authentication error: {0:?}")]
    Other(http::HeaderValue),
}

impl crate::IntoStatic for AuthError {
    type Output = AuthError;

    fn into_static(self) -> Self::Output {
        match self {
            AuthError::TokenExpired => AuthError::TokenExpired,
            AuthError::InvalidToken => AuthError::InvalidToken,
            AuthError::RefreshFailed => AuthError::RefreshFailed,
            AuthError::NotAuthenticated => AuthError::NotAuthenticated,
            AuthError::Other(header) => AuthError::Other(header),
        }
    }
}
