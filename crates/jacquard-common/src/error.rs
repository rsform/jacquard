//! Error types for XRPC client operations

use crate::xrpc::EncodeError;
use alloc::boxed::Box;
use alloc::string::ToString;
use bytes::Bytes;
use smol_str::SmolStr;

#[cfg(feature = "std")]
use miette::Diagnostic;

/// Boxed error type for wrapping arbitrary errors
pub type BoxError = Box<dyn core::error::Error + Send + Sync + 'static>;

/// Client error type for all XRPC client operations
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
#[error("{kind}")]
pub struct ClientError {
    #[cfg_attr(feature = "std", diagnostic_source)]
    kind: ClientErrorKind,
    #[source]
    source: Option<BoxError>,
    #[cfg_attr(feature = "std", help)]
    help: Option<SmolStr>,
    context: Option<SmolStr>,
    url: Option<SmolStr>,
    details: Option<SmolStr>,
    location: Option<SmolStr>,
}

/// Error categories for client operations
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
#[non_exhaustive]
pub enum ClientErrorKind {
    /// HTTP transport error (connection, timeout, etc.)
    #[error("transport error")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::client::transport)))]
    Transport,

    /// Request validation/construction failed
    #[error("invalid request: {0}")]
    #[cfg_attr(
        feature = "std",
        diagnostic(
            code(jacquard::client::invalid_request),
            help("check request parameters and format")
        )
    )]
    InvalidRequest(SmolStr),

    /// Request serialization failed
    #[error("encode error: {0}")]
    #[cfg_attr(
        feature = "std",
        diagnostic(
            code(jacquard::client::encode),
            help("check request body format and encoding")
        )
    )]
    Encode(SmolStr),

    /// Response deserialization failed
    #[error("decode error: {0}")]
    #[cfg_attr(
        feature = "std",
        diagnostic(
            code(jacquard::client::decode),
            help("check response format and encoding")
        )
    )]
    Decode(SmolStr),

    /// HTTP error response (non-200 status)
    #[error("HTTP {status}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::client::http)))]
    Http {
        /// HTTP status code
        status: http::StatusCode,
    },

    /// Authentication/authorization error
    #[error("auth error: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::client::auth)))]
    Auth(AuthError),

    /// Identity resolution error (handle→DID, DID→Doc)
    #[error("identity resolution failed")]
    #[cfg_attr(
        feature = "std",
        diagnostic(
            code(jacquard::client::identity_resolution),
            help("check handle/DID is valid and network is accessible")
        )
    )]
    IdentityResolution,

    /// Storage/persistence error
    #[error("storage error")]
    #[cfg_attr(
        feature = "std",
        diagnostic(
            code(jacquard::client::storage),
            help("check storage backend is accessible and has sufficient permissions")
        )
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

    /// Returns the HTTP status code if this is an `Http` error kind.
    pub fn status(&self) -> Option<http::StatusCode> {
        match &self.kind {
            ClientErrorKind::Http { status } => Some(*status),
            _ => None,
        }
    }

    /// Returns true if this is an authentication error (typed `Auth` kind or HTTP 401).
    pub fn is_auth(&self) -> bool {
        matches!(self.kind, ClientErrorKind::Auth(_))
            || self.status() == Some(http::StatusCode::UNAUTHORIZED)
    }

    /// Returns true if this is an HTTP 404 response.
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(http::StatusCode::NOT_FOUND)
    }

    /// Returns true if this is an HTTP 409 conflict response.
    pub fn is_conflict(&self) -> bool {
        self.status() == Some(http::StatusCode::CONFLICT)
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

    /// Append additional context to existing context string.
    ///
    /// If context already exists, appends with ": " separator.
    /// If no context exists, sets it directly.
    pub fn append_context(mut self, additional: impl AsRef<str>) -> Self {
        self.context = Some(match self.context.take() {
            Some(existing) => smol_str::format_smolstr!("{}: {}", existing, additional.as_ref()),
            None => additional.as_ref().into(),
        });
        self
    }

    /// Add NSID context for XRPC operations.
    ///
    /// Appends the NSID in brackets to existing context, e.g. `"network timeout: [com.atproto.repo.getRecord]"`.
    pub fn for_nsid(self, nsid: &str) -> Self {
        self.append_context(smol_str::format_smolstr!("[{}]", nsid))
    }

    /// Add collection context for record operations.
    ///
    /// Use this when a record operation fails to indicate the target collection.
    pub fn for_collection(self, operation: &str, collection_nsid: &str) -> Self {
        self.append_context(smol_str::format_smolstr!(
            "{} [{}]",
            operation,
            collection_nsid
        ))
    }

    // Constructors for each kind

    /// Create a transport error
    pub fn transport(source: impl core::error::Error + Send + Sync + 'static) -> Self {
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
    pub fn identity_resolution(source: impl core::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ClientErrorKind::IdentityResolution, Some(Box::new(source)))
    }

    /// Create a storage error
    pub fn storage(source: impl core::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ClientErrorKind::Storage, Some(Box::new(source)))
    }
}

/// Result type for client operations
pub type XrpcResult<T> = Result<T, ClientError>;

// ============================================================================
// Old error types (deprecated)
// ============================================================================

/// Response deserialization errors
///
/// Preserves detailed error information from various deserialization backends.
/// Can be converted to string for serialization while maintaining the full error context.
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
#[non_exhaustive]
pub enum DecodeError {
    /// JSON deserialization failed
    #[error("Failed to deserialize JSON: {0}")]
    Json(
        #[from]
        #[source]
        serde_json::Error,
    ),
    /// CBOR deserialization failed (local I/O)
    #[cfg(feature = "std")]
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
        serde_ipld_dagcbor::DecodeError<core::convert::Infallible>,
    ),
    /// CBOR header deserialization failed (framed WebSocket messages)
    #[cfg(all(feature = "websocket", feature = "std"))]
    #[error("Failed to deserialize cbor header: {0}")]
    CborHeader(
        #[from]
        #[source]
        ciborium::de::Error<std::io::Error>,
    ),

    /// CBOR header deserialization failed (framed WebSocket messages, no_std)
    #[cfg(all(feature = "websocket", not(feature = "std")))]
    #[error("Failed to deserialize cbor header: {0}")]
    CborHeader(
        #[from]
        #[source]
        ciborium::de::Error<core::convert::Infallible>,
    ),

    /// Unknown event type in framed message
    #[cfg(feature = "websocket")]
    #[error("Unknown event type: {0}")]
    UnknownEventType(smol_str::SmolStr),
}

/// HTTP error response (non-200 status codes outside of XRPC error handling)
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
pub struct HttpError {
    /// HTTP status code
    pub status: http::StatusCode,
    /// Response body if available
    pub body: Option<Bytes>,
}

impl core::fmt::Display for HttpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "HTTP {}", self.status)?;
        if let Some(body) = &self.body {
            if let Ok(s) = core::str::from_utf8(body) {
                write!(f, ":\n{}", s)?;
            }
        }
        Ok(())
    }
}

/// Authentication and authorization errors
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
#[non_exhaustive]
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

    /// DPoP proof construction failed (key or signing issue)
    #[error("DPoP proof construction failed")]
    DpopProofFailed,

    /// DPoP nonce retry failed (server rejected proof even after nonce update)
    #[error("DPoP nonce negotiation failed")]
    DpopNonceFailed,

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
            AuthError::DpopProofFailed => AuthError::DpopProofFailed,
            AuthError::DpopNonceFailed => AuthError::DpopNonceFailed,
            AuthError::Other(header) => AuthError::Other(header),
        }
    }
}

// ============================================================================
// Conversions from old to new
// ============================================================================

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

#[cfg(feature = "std")]
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

impl From<serde_ipld_dagcbor::DecodeError<core::convert::Infallible>> for ClientError {
    fn from(e: serde_ipld_dagcbor::DecodeError<core::convert::Infallible>) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(ClientErrorKind::Decode(msg), Some(Box::new(e)))
            .with_context("DAG-CBOR deserialization failed (in-memory)")
    }
}

#[cfg(all(feature = "websocket", feature = "std"))]
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

// fluent_uri parse errors
impl From<crate::deps::fluent_uri::ParseError> for ClientError {
    fn from(e: crate::deps::fluent_uri::ParseError) -> Self {
        Self::invalid_request(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn client_error_status_from_http() {
        let err = ClientError::http(StatusCode::CONFLICT, None);
        assert_eq!(err.status(), Some(StatusCode::CONFLICT));
    }

    #[test]
    fn client_error_status_none_for_non_http() {
        let err = ClientError::invalid_request("bad");
        assert_eq!(err.status(), None);
    }

    #[test]
    fn client_error_is_auth_typed() {
        let err = ClientError::auth(AuthError::TokenExpired);
        assert!(err.is_auth());
    }

    #[test]
    fn client_error_is_auth_http_401() {
        let err = ClientError::http(StatusCode::UNAUTHORIZED, None);
        assert!(err.is_auth());
    }

    #[test]
    fn client_error_is_not_found() {
        assert!(ClientError::http(StatusCode::NOT_FOUND, None).is_not_found());
        assert!(!ClientError::http(StatusCode::BAD_REQUEST, None).is_not_found());
    }

    #[test]
    fn client_error_is_conflict() {
        assert!(ClientError::http(StatusCode::CONFLICT, None).is_conflict());
        assert!(!ClientError::http(StatusCode::NOT_FOUND, None).is_conflict());
    }
}
