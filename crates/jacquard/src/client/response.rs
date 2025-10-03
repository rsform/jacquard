use bytes::Bytes;
use jacquard_common::IntoStatic;
use jacquard_common::types::xrpc::XrpcRequest;
use std::marker::PhantomData;

/// XRPC response wrapper that owns the response buffer
///
/// Allows borrowing from the buffer when parsing to avoid unnecessary allocations.
pub struct Response<R: XrpcRequest> {
    buffer: Bytes,
    _marker: PhantomData<R>,
}

impl<R: XrpcRequest> Response<R> {
    /// Create a new response from a buffer
    pub fn new(buffer: Bytes) -> Self {
        Self {
            buffer,
            _marker: PhantomData,
        }
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

        let output = parse_output::<R>(&self.buffer);
        if let Ok(output) = output {
            Ok(output)
        } else {
            // Try to parse as error
            match parse_error::<R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error)),
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

        let output = parse_output::<R>(&self.buffer);
        if let Ok(output) = output {
            Ok(output.into_static())
        } else {
            // Try to parse as error
            match parse_error::<R>(&self.buffer) {
                Ok(error) => Err(XrpcError::Xrpc(error.into_static())),
                Err(e) => Err(XrpcError::Decode(e)),
            }
        }
    }

    /// Get the raw buffer
    pub fn buffer(&self) -> &Bytes {
        &self.buffer
    }
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum XrpcError<E: std::error::Error + IntoStatic> {
    #[error("XRPC error: {0}")]
    Xrpc(E),
    #[error("Failed to decode response: {0}")]
    Decode(#[from] serde_json::Error),
}
