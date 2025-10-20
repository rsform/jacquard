//! Error types for XRPC client operations

use crate::xrpc::EncodeError;
use bytes::Bytes;
use smol_str::SmolStr;

/// Boxed error type for wrapping arbitrary errors
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Client error type for all XRPC client operations
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{kind}")]
pub struct ClientError {
    #[diagnostic_source]
    kind: ClientErrorKind,
    #[source]
    source: Option<BoxError>,
    #[help]
    help: Option<SmolStr>,
    context: Option<SmolStr>,
    url: Option<SmolStr>,
    details: Option<SmolStr>,
    location: Option<SmolStr>,
}

/// Error categories for client operations
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ClientErrorKind {
    /// HTTP transport error (connection, timeout, etc.)
    #[error("transport error")]
    #[diagnostic(code(jacquard::client::transport))]
    Transport,

    /// Request validation/construction failed
    #[error("invalid request: {0}")]
    #[diagnostic(
        code(jacquard::client::invalid_request),
        help("check request parameters and format")
    )]
    InvalidRequest(SmolStr),

    /// Request serialization failed
    #[error("encode error: {0}")]
    #[diagnostic(
        code(jacquard::client::encode),
        help("check request body format and encoding")
    )]
    Encode(SmolStr),

    /// Response deserialization failed
    #[error("decode error: {0}")]
    #[diagnostic(
        code(jacquard::client::decode),
        help("check response format and encoding")
    )]
    Decode(SmolStr),

    /// HTTP error response (non-200 status)
    #[error("HTTP {status}")]
    #[diagnostic(code(jacquard::client::http))]
    Http {
        /// HTTP status code
        status: http::StatusCode,
    },

    /// Authentication/authorization error
    #[error("auth error: {0}")]
    #[diagnostic(code(jacquard::client::auth))]
    Auth(AuthError),

    /// Identity resolution error (handle→DID, DID→Doc)
    #[error("identity resolution failed")]
    #[diagnostic(
        code(jacquard::client::identity_resolution),
        help("check handle/DID is valid and network is accessible")
    )]
    IdentityResolution,

    /// Storage/persistence error
    #[error("storage error")]
    #[diagnostic(
        code(jacquard::client::storage),
        help("check storage backend is accessible and has sufficient permissions")
    )]
    Storage,
}

impl ClientError {
    /// Create a new error with the given kind and optional source
    pub fn new(kind: ClientErrorKind, source: Option<BoxError>) -> Self {
        Self {
            kind,
            source,
            help: None,
            context: None,
            url: None,
            details: None,
            location: None,
        }
    }

    /// Get the error kind
    pub fn kind(&self) -> &ClientErrorKind {
        &self.kind
    }

    /// Get the source error if present
    pub fn source_err(&self) -> Option<&BoxError> {
        self.source.as_ref()
    }

    /// Get the context string if present
    pub fn context(&self) -> Option<&str> {
        self.context.as_ref().map(|s| s.as_str())
    }

    /// Get the URL if present
    pub fn url(&self) -> Option<&str> {
        self.url.as_ref().map(|s| s.as_str())
    }

    /// Get the details if present
    pub fn details(&self) -> Option<&str> {
        self.details.as_ref().map(|s| s.as_str())
    }

    /// Get the location if present
    pub fn location(&self) -> Option<&str> {
        self.location.as_ref().map(|s| s.as_str())
    }

    /// Add help text to this error
    pub fn with_help(mut self, help: impl Into<SmolStr>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add context to this error
    pub fn with_context(mut self, context: impl Into<SmolStr>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Add URL to this error
    pub fn with_url(mut self, url: impl Into<SmolStr>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add details to this error
    pub fn with_details(mut self, details: impl Into<SmolStr>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add location to this error
    pub fn with_location(mut self, location: impl Into<SmolStr>) -> Self {
        self.location = Some(location.into());
        self
    }

    // Constructors for each kind

    /// Create a transport error
    pub fn transport(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ClientErrorKind::Transport, Some(Box::new(source)))
    }

    /// Create an invalid request error
    pub fn invalid_request(msg: impl Into<SmolStr>) -> Self {
        Self::new(ClientErrorKind::InvalidRequest(msg.into()), None)
    }

    /// Create an encode error
    pub fn encode(msg: impl Into<SmolStr>) -> Self {
        Self::new(ClientErrorKind::Encode(msg.into()), None)
    }

    /// Create a decode error
    pub fn decode(msg: impl Into<SmolStr>) -> Self {
        Self::new(ClientErrorKind::Decode(msg.into()), None)
    }

    /// Create an HTTP error with status code and optional body
    pub fn http(status: http::StatusCode, body: Option<Bytes>) -> Self {
        let http_err = HttpError { status, body };
        Self::new(ClientErrorKind::Http { status }, Some(Box::new(http_err)))
    }

    /// Create an authentication error
    pub fn auth(auth_error: AuthError) -> Self {
        Self::new(ClientErrorKind::Auth(auth_error), None)
    }

    /// Create an identity resolution error
    pub fn identity_resolution(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ClientErrorKind::IdentityResolution, Some(Box::new(source)))
    }

    /// Create a storage error
    pub fn storage(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ClientErrorKind::Storage, Some(Box::new(source)))
    }
}

/// Result type for client operations
pub type XrpcResult<T> = std::result::Result<T, ClientError>;

// ============================================================================
// Old error types (deprecated)
// ============================================================================

/// Transport-level errors that occur during HTTP communication
// #[deprecated(since = "0.8.0", note = "Use ClientError::transport() instead")]
// #[derive(Debug, thiserror::Error, miette::Diagnostic)]
// pub enum TransportError {
//     /// Failed to establish connection to server
//     #[error("Connection error: {0}")]
//     Connect(String),

//     /// Request timed out
//     #[error("Request timeout")]
//     Timeout,

//     /// Request construction failed (malformed URI, headers, etc.)
//     #[error("Invalid request: {0}")]
//     InvalidRequest(String),

//     /// Other transport error
//     #[error("Transport error: {0}")]
//     Other(Box<dyn std::error::Error + Send + Sync>),
// }

/// Response deserialization errors
///
/// Preserves detailed error information from various deserialization backends.
/// Can be converted to string for serialization while maintaining the full error context.
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
    /// DAG-CBOR deserialization failed (in-memory, e.g., WebSocket frames)
    #[error("Failed to deserialize DAG-CBOR: {0}")]
    DagCborInfallible(
        #[from]
        #[source]
        serde_ipld_dagcbor::DecodeError<std::convert::Infallible>,
    ),
    /// CBOR header deserialization failed (framed WebSocket messages)
    #[cfg(feature = "websocket")]
    #[error("Failed to deserialize cbor header: {0}")]
    CborHeader(
        #[from]
        #[source]
        ciborium::de::Error<std::io::Error>,
    ),

    /// Unknown event type in framed message
    #[cfg(feature = "websocket")]
    #[error("Unknown event type: {0}")]
    UnknownEventType(smol_str::SmolStr),
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

// ============================================================================
// Conversions from old to new
// ============================================================================

#[allow(deprecated)]
// impl From<TransportError> for ClientError {
//     fn from(e: TransportError) -> Self {
//         Self::transport(e)
//     }
// }

impl From<DecodeError> for ClientError {
    fn from(e: DecodeError) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("response deserialization failed")
    }
}

impl From<HttpError> for ClientError {
    fn from(e: HttpError) -> Self {
        Self::http(e.status, e.body)
    }
}

impl From<AuthError> for ClientError {
    fn from(e: AuthError) -> Self {
        Self::auth(e)
    }
}

impl From<EncodeError> for ClientError {
    fn from(e: EncodeError) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Encode(msg), Some(Box::new(e)))
            .with_context("request encoding failed")
    }
}

// Platform-specific conversions
#[cfg(feature = "reqwest-client")]
impl From<reqwest::Error> for ClientError {
    #[cfg(not(target_arch = "wasm32"))]
    fn from(e: reqwest::Error) -> Self {
        Self::transport(e)
    }

    #[cfg(target_arch = "wasm32")]
    fn from(e: reqwest::Error) -> Self {
        Self::transport(e)
    }
}

// Serde error conversions
impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("JSON deserialization failed")
    }
}

impl From<serde_ipld_dagcbor::DecodeError<std::io::Error>> for ClientError {
    fn from(e: serde_ipld_dagcbor::DecodeError<std::io::Error>) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("DAG-CBOR deserialization failed (local I/O)")
    }
}

impl From<serde_ipld_dagcbor::DecodeError<HttpError>> for ClientError {
    fn from(e: serde_ipld_dagcbor::DecodeError<HttpError>) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("DAG-CBOR deserialization failed (remote)")
    }
}

impl From<serde_ipld_dagcbor::DecodeError<std::convert::Infallible>> for ClientError {
    fn from(e: serde_ipld_dagcbor::DecodeError<std::convert::Infallible>) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("DAG-CBOR deserialization failed (in-memory)")
    }
}

#[cfg(feature = "websocket")]
impl From<ciborium::de::Error<std::io::Error>> for ClientError {
    fn from(e: ciborium::de::Error<std::io::Error>) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("CBOR header deserialization failed")
    }
}

// Session store errors
impl From<crate::session::SessionStoreError> for ClientError {
    fn from(e: crate::session::SessionStoreError) -> Self {
        Self::storage(e)
    }
}

// URL parse errors
impl From<url::ParseError> for ClientError {
    fn from(e: url::ParseError) -> Self {
        Self::invalid_request(e.to_string())
    }
}
