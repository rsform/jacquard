use bytes::Bytes;
use http::StatusCode;
use jacquard_common::IntoStatic;
use jacquard_common::types::xrpc::XrpcRequest;
use serde::Deserialize;
use std::marker::PhantomData;

use super::error::AuthError;

/// XRPC response wrapper that owns the response buffer
///
/// Allows borrowing from the buffer when parsing to avoid unnecessary allocations.
pub struct Response<R: XrpcRequest> {
    buffer: Bytes,
    status: StatusCode,
    _marker: PhantomData<R>,
}

impl<R: XrpcRequest> Response<R> {
    /// Create a new response from a buffer and status code
    pub fn new(buffer: Bytes, status: StatusCode) -> Self {
        Self {
            buffer,
            status,
            _marker: PhantomData,
        }
    }

    /// Get the HTTP status code
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Parse the response, borrowing from the internal buffer
    pub fn parse(&self) -> Result<R::Output<'_>, XrpcError<R::Err<'_>>> {
        // Use a helper to make lifetime inference work
        fn parse_output<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Output<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        fn parse_error<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Err<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match parse_output::<R>(&self.buffer) {
                Ok(output) => Ok(output),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match parse_error::<R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(generic) => {
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                                _ => Err(XrpcError::Generic(generic)),
                            }
                        }
                        Err(e) => Err(XrpcError::Decode(e)),
                    }
                }
            }
        // 401: always auth error
        } else {
            match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(generic) => {
                    match generic.error.as_str() {
                        "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                        "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                        _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                    }
                }
                Err(e) => Err(XrpcError::Decode(e)),
            }
        }
    }

    /// Parse the response into an owned output
    pub fn into_output(self) -> Result<R::Output<'static>, XrpcError<R::Err<'static>>>
    where
        for<'a> R::Output<'a>: IntoStatic<Output = R::Output<'static>>,
        for<'a> R::Err<'a>: IntoStatic<Output = R::Err<'static>>,
    {
        // Use a helper to make lifetime inference work
        fn parse_output<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Output<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        fn parse_error<'b, R: XrpcRequest>(
            buffer: &'b [u8],
        ) -> Result<R::Err<'b>, serde_json::Error> {
            serde_json::from_slice(buffer)
        }

        // 200: parse as output
        if self.status.is_success() {
            match parse_output::<R>(&self.buffer) {
                Ok(output) => Ok(output.into_static()),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        // 400: try typed XRPC error, fallback to generic error
        } else if self.status.as_u16() == 400 {
            match parse_error::<R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error.into_static())),
                Err(_) => {
                    // Fallback to generic error (InvalidRequest, ExpiredToken, etc.)
                    match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                        Ok(generic) => {
                            // Map auth-related errors to AuthError
                            match generic.error.as_str() {
                                "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                                "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                                _ => Err(XrpcError::Generic(generic)),
                            }
                        }
                        Err(e) => Err(XrpcError::Decode(e)),
                    }
                }
            }
        // 401: always auth error
        } else {
            match serde_json::from_slice::<GenericXrpcError>(&self.buffer) {
                Ok(generic) => {
                    match generic.error.as_str() {
                        "ExpiredToken" => Err(XrpcError::Auth(AuthError::TokenExpired)),
                        "InvalidToken" => Err(XrpcError::Auth(AuthError::InvalidToken)),
                        _ => Err(XrpcError::Auth(AuthError::NotAuthenticated)),
                    }
                }
                Err(e) => Err(XrpcError::Decode(e)),
            }
        }
    }

    /// Get the raw buffer
    pub fn buffer(&self) -> &Bytes {
        &self.buffer
    }
}

/// Generic XRPC error format (for InvalidRequest, etc.)
#[derive(Debug, Clone, Deserialize)]
pub struct GenericXrpcError {
    pub error: String,
    pub message: Option<String>,
}

impl std::fmt::Display for GenericXrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(msg) = &self.message {
            write!(f, "{}: {}", self.error, msg)
        } else {
            write!(f, "{}", self.error)
        }
    }
}

impl std::error::Error for GenericXrpcError {}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum XrpcError<E: std::error::Error + IntoStatic> {
    /// Typed XRPC error from the endpoint's error enum
    #[error("XRPC error: {0}")]
    Xrpc(E),

    /// Authentication error (ExpiredToken, InvalidToken, etc.)
    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    /// Generic XRPC error (InvalidRequest, etc.)
    #[error("XRPC error: {0}")]
    Generic(GenericXrpcError),

    /// Failed to decode response
    #[error("Failed to decode response: {0}")]
    Decode(#[from] serde_json::Error),
}
