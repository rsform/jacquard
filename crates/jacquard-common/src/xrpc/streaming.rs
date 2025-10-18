//! Streaming support for XRPC requests and responses

use crate::{IntoStatic, StreamError, stream::ByteStream, xrpc::XrpcRequest};
use bytes::Bytes;
use http::StatusCode;
use n0_future::{StreamExt, TryStreamExt, stream::Boxed};
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::{marker::PhantomData, pin::Pin};

pub trait XrpcProcedureStream {
    /// The NSID for this XRPC method
    const NSID: &'static str;
    /// The upload encoding
    const ENCODING: &'static str;

    type Frame<'de>;

    type Request: XrpcRequest;

    /// Response type returned from the XRPC call (marker struct)
    type Response: XrpcStreamResp;

    fn encode_frame<'de>(data: Self::Frame<'de>) -> Result<Bytes, StreamError>
    where
        Self::Frame<'de>: Serialize,
    {
        Ok(Bytes::from_owner(
            serde_ipld_dagcbor::to_vec(&data).map_err(StreamError::encode)?,
        ))
    }

    /// Decode the request body for procedures.
    ///
    /// Default implementation deserializes from CBOR. Override for non-CBOR encodings.
    fn decode_frame<'de>(frame: &'de [u8]) -> Result<Self::Frame<'de>, StreamError>
    where
        Self::Frame<'de>: Deserialize<'de>,
    {
        Ok(serde_ipld_dagcbor::from_slice(frame).map_err(StreamError::decode)?)
    }
}

/// Trait for XRPC Response types
///
/// It mirrors the NSID and carries the encoding types as well as Output (success) and Err types
pub trait XrpcStreamResp {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// Output encoding (MIME type)
    const ENCODING: &'static str;

    /// Response output type
    type Frame<'de>: IntoStatic;

    fn encode_frame<'de>(data: Self::Frame<'de>) -> Result<Bytes, StreamError>
    where
        Self::Frame<'de>: Serialize,
    {
        Ok(Bytes::from_owner(
            serde_ipld_dagcbor::to_vec(&data).map_err(StreamError::encode)?,
        ))
    }

    /// Decode the request body for procedures.
    ///
    /// Default implementation deserializes from CBOR. Override for non-CBOR encodings.
    ///
    /// TODO: make this handle when frames are fragmented?
    fn decode_frame<'de>(frame: &'de [u8]) -> Result<Self::Frame<'de>, StreamError>
    where
        Self::Frame<'de>: Deserialize<'de>,
    {
        Ok(serde_ipld_dagcbor::from_slice(frame).map_err(StreamError::decode)?)
    }
}

#[repr(transparent)]
pub struct XrpcStreamFrame<F = ()> {
    pub buffer: Bytes,
    _marker: PhantomData<F>,
}

impl XrpcStreamFrame {
    pub fn new(buffer: Bytes) -> Self {
        Self {
            buffer,
            _marker: PhantomData,
        }
    }
}

impl<F> XrpcStreamFrame<F> {
    pub fn new_typed<G>(buffer: Bytes) -> Self {
        Self {
            buffer,
            _marker: PhantomData,
        }
    }
}

/// Dumb file upload stream
///
/// Unavailable on wasm due to use of tokio I/O
#[cfg(not(target_arch = "wasm32"))]
pub async fn upload_stream(file: impl AsRef<Path>) -> Result<XrpcProcedureSend, tokio::io::Error> {
    use tokio_util::io::ReaderStream;

    let file = tokio::fs::File::open(file).await?;
    let reader = ReaderStream::new(file);
    let stream = reader
        .map(|b| match b {
            Ok(bytes) => Ok(XrpcStreamFrame::new(bytes)),
            Err(err) => Err(StreamError::transport(err)),
        })
        .boxed();

    Ok(XrpcProcedureSend(stream))
}

/// Encode a stream of items into the corresponding XRPC procedure stream.
pub fn encode_stream<P: XrpcProcedureStream + 'static>(
    s: Boxed<P::Frame<'static>>,
) -> XrpcProcedureSend<P::Frame<'static>>
where
    <P as XrpcProcedureStream>::Frame<'static>: Serialize,
{
    let stream = s
        .map(|f| P::encode_frame(f).map(|b| XrpcStreamFrame::new_typed::<P::Frame<'_>>(b)))
        .boxed();

    XrpcProcedureSend(stream)
}

/// Sending stream for streaming XRPC procedure uplink.
pub struct XrpcProcedureSend<F = ()>(pub Boxed<Result<XrpcStreamFrame<F>, StreamError>>);

/// Sink half of XRPC procedure uplink stream, for use in pipe scenarios.
pub struct XrpcProcedureSink<F = ()>(
    pub Pin<Box<dyn n0_future::Sink<XrpcStreamFrame<F>, Error = StreamError> + Send>>,
);

pub struct XrpcResponseStream<F = ()> {
    parts: http::response::Parts,
    body: Boxed<Result<XrpcStreamFrame<F>, StreamError>>,
}

impl XrpcResponseStream {
    pub fn from_bytestream(StreamingResponse { parts, body }: StreamingResponse) -> Self {
        Self {
            parts,
            body: body
                .into_inner()
                .map_ok(|b| XrpcStreamFrame::new(b))
                .boxed(),
        }
    }

    pub fn from_parts(parts: http::response::Parts, body: ByteStream) -> Self {
        Self {
            parts,
            body: body
                .into_inner()
                .map_ok(|b| XrpcStreamFrame::new(b))
                .boxed(),
        }
    }

    pub fn into_parts(self) -> (http::response::Parts, ByteStream) {
        (
            self.parts,
            ByteStream::new(self.body.map_ok(|f| f.buffer).boxed()),
        )
    }

    pub fn into_bytestream(self) -> ByteStream {
        ByteStream::new(self.body.map_ok(|f| f.buffer).boxed())
    }
}

impl<F: XrpcStreamResp> XrpcResponseStream<F> {
    pub fn from_stream(StreamingResponse { parts, body }: StreamingResponse) -> Self {
        Self {
            parts,
            body: body
                .into_inner()
                .map_ok(|b| XrpcStreamFrame::new_typed::<F::Frame<'_>>(b))
                .boxed(),
        }
    }

    pub fn from_typed_parts(parts: http::response::Parts, body: ByteStream) -> Self {
        Self {
            parts,
            body: body
                .into_inner()
                .map_ok(|b| XrpcStreamFrame::new_typed::<F::Frame<'_>>(b))
                .boxed(),
        }
    }
}

impl<F: XrpcStreamResp + 'static> XrpcResponseStream<F> {
    pub fn into_bytestream(self) -> ByteStream {
        ByteStream::new(self.body.map_ok(|f| f.buffer).boxed())
    }
}

/// HTTP streaming response
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
