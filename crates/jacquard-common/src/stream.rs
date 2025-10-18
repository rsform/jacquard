//! Stream abstractions for HTTP request/response bodies
//!
//! This module provides platform-agnostic streaming types for handling large
//! payloads efficiently without loading everything into memory.
//!
//! # Features
//!
//! - [`ByteStream`]: Streaming response bodies
//! - [`ByteSink`]: Streaming request bodies
//! - [`StreamError`]: Concrete error type for streaming operations
//!
//! # Platform Support
//!
//! Uses `n0-future` for platform-agnostic async streams that work on both
//! native and WASM targets without requiring `Send` bounds on WASM.
//!
//! # Examples
//!
//! ## Streaming Download
//!
//! ```no_run
//! # #[cfg(all(feature = "streaming", feature = "reqwest-client"))]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use jacquard_common::http_client::{HttpClient, HttpClientExt};
//! use futures_lite::StreamExt;
//!
//! let client = reqwest::Client::new();
//! let request = http::Request::builder()
//!     .uri("https://example.com/large-file")
//!     .body(vec![])
//!     .unwrap();
//!
//! let response = client.send_http_streaming(request).await?;
//! let (_parts, body) = response.into_parts();
//! let mut stream = Box::pin(body.into_inner());
//!
//! // Use futures_lite::StreamExt for iteration
//! while let Some(chunk) = stream.as_mut().try_next().await? {
//!     // Process chunk without loading entire file into memory
//! }
//! # Ok(())
//! # }
//! ```

use std::error::Error;
use std::fmt;

/// Boxed error type for streaming operations
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Error type for streaming operations
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub struct StreamError {
    kind: StreamErrorKind,
    #[source]
    source: Option<BoxError>,
}

/// Categories of streaming errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorKind {
    /// Network or I/O error
    Transport,
    /// Stream or connection closed
    Closed,
    /// Protocol violation or framing error
    Protocol,
    /// Message deserialization failed
    Decode,
    /// Message serialization failed
    Encode,
    /// Wrong message format (e.g., text frame when expecting binary)
    WrongMessageFormat,
}

impl StreamError {
    /// Create a new streaming error
    pub fn new(kind: StreamErrorKind, source: Option<BoxError>) -> Self {
        Self { kind, source }
    }

    /// Get the error kind
    pub fn kind(&self) -> &StreamErrorKind {
        &self.kind
    }

    /// Get the underlying error source
    pub fn source(&self) -> Option<&BoxError> {
        self.source.as_ref()
    }

    /// Create a "connection closed" error
    pub fn closed() -> Self {
        Self {
            kind: StreamErrorKind::Closed,
            source: None,
        }
    }

    /// Create a transport error with source
    pub fn transport(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind: StreamErrorKind::Transport,
            source: Some(Box::new(source)),
        }
    }

    /// Create a protocol error
    pub fn protocol(msg: impl Into<String>) -> Self {
        Self {
            kind: StreamErrorKind::Protocol,
            source: Some(msg.into().into()),
        }
    }

    /// Create a decode error with source
    pub fn decode(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind: StreamErrorKind::Decode,
            source: Some(Box::new(source)),
        }
    }

    /// Create an encode error with source
    pub fn encode(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind: StreamErrorKind::Encode,
            source: Some(Box::new(source)),
        }
    }

    /// Create a wrong message format error
    pub fn wrong_message_format(msg: impl Into<String>) -> Self {
        Self {
            kind: StreamErrorKind::WrongMessageFormat,
            source: Some(msg.into().into()),
        }
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            StreamErrorKind::Transport => write!(f, "Transport error"),
            StreamErrorKind::Closed => write!(f, "Stream closed"),
            StreamErrorKind::Protocol => write!(f, "Protocol error"),
            StreamErrorKind::Decode => write!(f, "Decode error"),
            StreamErrorKind::Encode => write!(f, "Encode error"),
            StreamErrorKind::WrongMessageFormat => write!(f, "Wrong message format"),
        }?;

        if let Some(source) = &self.source {
            write!(f, ": {}", source)?;
        }

        Ok(())
    }
}

use bytes::Bytes;
use n0_future::stream::Boxed;

/// Platform-agnostic byte stream abstraction
pub struct ByteStream {
    inner: Boxed<Result<Bytes, StreamError>>,
}

impl ByteStream {
    /// Create a new byte stream from any compatible stream
    pub fn new<S>(stream: S) -> Self
    where
        S: n0_future::Stream<Item = Result<Bytes, StreamError>> + Unpin + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Check if stream is known to be empty (always false for dynamic streams)
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Convert into the inner boxed stream
    pub fn into_inner(self) -> Boxed<Result<Bytes, StreamError>> {
        self.inner
    }

    /// Split this stream into two streams that both receive all chunks
    ///
    /// Chunks are cloned (cheaply via Bytes rc). Spawns a forwarder task.
    /// Both returned streams will receive all chunks from the original stream.
    /// The forwarder continues as long as at least one stream is alive.
    /// If the underlying stream errors, both teed streams will end.
    pub fn tee(self) -> (ByteStream, ByteStream) {
        use futures::channel::mpsc;
        use n0_future::StreamExt as _;

        let (tx1, rx1) = mpsc::unbounded();
        let (tx2, rx2) = mpsc::unbounded();

        n0_future::task::spawn(async move {
            let mut stream = self.inner;
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        // Clone chunk (cheap - Bytes is rc'd)
                        let chunk2 = chunk.clone();

                        // Send to both channels, continue if at least one succeeds
                        let send1 = tx1.unbounded_send(Ok(chunk));
                        let send2 = tx2.unbounded_send(Ok(chunk2));

                        // Only stop if both channels are closed
                        if send1.is_err() && send2.is_err() {
                            break;
                        }
                    }
                    Err(_e) => {
                        // Underlying stream errored, stop forwarding.
                        // Both channels will close, ending both streams.
                        break;
                    }
                }
            }
        });

        (ByteStream::new(rx1), ByteStream::new(rx2))
    }
}

impl fmt::Debug for ByteStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteStream").finish_non_exhaustive()
    }
}

/// Platform-agnostic byte sink abstraction
pub struct ByteSink {
    inner: Box<dyn n0_future::Sink<Bytes, Error = StreamError>>,
}

impl ByteSink {
    /// Create a new byte sink from any compatible sink
    pub fn new<S>(sink: S) -> Self
    where
        S: n0_future::Sink<Bytes, Error = StreamError> + 'static,
    {
        Self {
            inner: Box::new(sink),
        }
    }

    /// Convert into the inner boxed sink
    pub fn into_inner(self) -> Box<dyn n0_future::Sink<Bytes, Error = StreamError>> {
        self.inner
    }
}

impl fmt::Debug for ByteSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteSink").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn stream_error_carries_kind_and_source() {
        let source = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe closed");
        let err = StreamError::new(StreamErrorKind::Transport, Some(Box::new(source)));

        assert_eq!(err.kind(), &StreamErrorKind::Transport);
        assert!(err.source().is_some());
        assert_eq!(format!("{}", err), "Transport error: pipe closed");
    }

    #[test]
    fn stream_error_without_source() {
        let err = StreamError::closed();

        assert_eq!(err.kind(), &StreamErrorKind::Closed);
        assert!(err.source().is_none());
    }

    #[tokio::test]
    async fn byte_stream_can_be_created() {
        use futures::stream;

        let data = vec![Ok(Bytes::from("hello")), Ok(Bytes::from(" world"))];
        let stream = stream::iter(data);

        let byte_stream = ByteStream::new(stream);
        assert!(!byte_stream.is_empty());
    }
}
