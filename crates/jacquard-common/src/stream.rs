//! Stream abstractions for HTTP request/response bodies

use std::error::Error;
use std::fmt;

/// Boxed error type for streaming operations
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Error type for streaming operations
#[derive(Debug)]
pub struct StreamError {
    kind: StreamErrorKind,
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
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            StreamErrorKind::Transport => write!(f, "Transport error"),
            StreamErrorKind::Closed => write!(f, "Stream closed"),
            StreamErrorKind::Protocol => write!(f, "Protocol error"),
        }?;

        if let Some(source) = &self.source {
            write!(f, ": {}", source)?;
        }

        Ok(())
    }
}

impl Error for StreamError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn Error + 'static))
    }
}

use bytes::Bytes;

/// Platform-agnostic byte stream abstraction
pub struct ByteStream {
    inner: Box<dyn n0_future::Stream<Item = Result<Bytes, StreamError>>>,
}

impl ByteStream {
    /// Create a new byte stream from any compatible stream
    pub fn new<S>(stream: S) -> Self
    where
        S: n0_future::Stream<Item = Result<Bytes, StreamError>> + 'static,
    {
        Self {
            inner: Box::new(stream),
        }
    }

    /// Check if stream is known to be empty (always false for dynamic streams)
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Convert into the inner boxed stream
    pub fn into_inner(self) -> Box<dyn n0_future::Stream<Item = Result<Bytes, StreamError>>> {
        self.inner
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

        let data = vec![
            Ok(Bytes::from("hello")),
            Ok(Bytes::from(" world")),
        ];
        let stream = stream::iter(data);

        let byte_stream = ByteStream::new(stream);
        assert!(!byte_stream.is_empty());
    }
}
