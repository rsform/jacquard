use bytes::Bytes;

/// Client error type
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

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum TransportError {
    #[error("Connection error: {0}")]
    Connect(String),

    #[error("Request timeout")]
    Timeout,

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Transport error: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

// Re-export EncodeError from common
pub use jacquard_common::types::xrpc::EncodeError;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum DecodeError {
    #[error("Failed to deserialize JSON: {0}")]
    Json(
        #[from]
        #[source]
        serde_json::Error,
    ),
    #[error("Failed to deserialize CBOR: {0}")]
    CborLocal(
        #[from]
        #[source]
        serde_ipld_dagcbor::DecodeError<std::io::Error>,
    ),
    #[error("Failed to deserialize CBOR: {0}")]
    CborRemote(
        #[from]
        #[source]
        serde_ipld_dagcbor::DecodeError<reqwest::Error>,
    ),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub struct HttpError {
    pub status: http::StatusCode,
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

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum AuthError {
    #[error("Access token expired")]
    TokenExpired,

    #[error("Invalid access token")]
    InvalidToken,

    #[error("Token refresh failed")]
    RefreshFailed,

    #[error("No authentication provided")]
    NotAuthenticated,
    #[error("Authentication error: {0:?}")]
    Other(http::HeaderValue),
}

pub type Result<T> = std::result::Result<T, ClientError>;

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
