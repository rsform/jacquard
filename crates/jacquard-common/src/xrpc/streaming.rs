//! Streaming support for XRPC requests and responses

use crate::stream::ByteStream;
use http::StatusCode;

/// XRPC streaming response
///
/// Similar to `Response<R>` but holds a streaming body instead of a buffer.
pub struct StreamingResponse {
    parts: http::response::Parts,
    body: ByteStream,
}

impl StreamingResponse {
    /// Create a new streaming response
    pub fn new(parts: http::response::Parts, body: ByteStream) -> Self {
        Self { parts, body }
    }

    /// Get the HTTP status code
    pub fn status(&self) -> StatusCode {
        self.parts.status
    }

    /// Get the response headers
    pub fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }

    /// Get the response version
    pub fn version(&self) -> http::Version {
        self.parts.version
    }

    /// Consume the response and return parts and body separately
    pub fn into_parts(self) -> (http::response::Parts, ByteStream) {
        (self.parts, self.body)
    }

    /// Get mutable access to the body stream
    pub fn body_mut(&mut self) -> &mut ByteStream {
        &mut self.body
    }

    /// Get a reference to the body stream
    pub fn body(&self) -> &ByteStream {
        &self.body
    }
}

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("status", &self.parts.status)
            .field("version", &self.parts.version)
            .field("headers", &self.parts.headers)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;

    #[test]
    fn streaming_response_holds_parts_and_body() {
        // Build parts from a Response and extract them
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(())
            .unwrap();
        let (parts, _) = response.into_parts();

        let stream = stream::iter(vec![Ok(Bytes::from("test"))]);
        let body = ByteStream::new(stream);

        let response = StreamingResponse::new(parts, body);
        assert_eq!(response.status(), StatusCode::OK);
    }
}
