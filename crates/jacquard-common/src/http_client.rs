//! Minimal HTTP client abstraction shared across crates.

use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

/// HTTP client trait for sending raw HTTP requests.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait HttpClient {
    /// Error type returned by the HTTP client
    type Error: std::error::Error + Display + Send + Sync + 'static;

    /// Send an HTTP request and return the response.
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>>;
}

#[cfg(feature = "streaming")]
use crate::stream::{ByteStream, StreamError};

/// Extension trait for HTTP client with streaming support
#[cfg(feature = "streaming")]
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait HttpClientExt: HttpClient {
    /// Send HTTP request and return streaming response
    fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = Result<http::Response<ByteStream>, Self::Error>>;

    /// Send HTTP request with streaming body and receive streaming response
    fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> impl Future<Output = Result<http::Response<ByteStream>, Self::Error>>
    where
        S: n0_future::Stream<Item = bytes::Bytes> + Send + 'static;
}

#[cfg(feature = "reqwest-client")]
impl HttpClient for reqwest::Client {
    type Error = reqwest::Error;

    async fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<Vec<u8>>, Self::Error> {
        // Convert http::Request to reqwest::Request
        let (parts, body) = request.into_parts();

        let mut req = self.request(parts.method, parts.uri.to_string()).body(body);

        // Copy headers
        for (name, value) in parts.headers.iter() {
            req = req.header(name.as_str(), value.as_bytes());
        }

        // Send request
        let resp = req.send().await?;

        // Convert reqwest::Response to http::Response
        let mut builder = http::Response::builder().status(resp.status());

        // Copy headers
        for (name, value) in resp.headers().iter() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }

        // Read body
        let body = resp.bytes().await?.to_vec();

        Ok(builder.body(body).expect("Failed to build response"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: HttpClient + Sync> HttpClient for Arc<T> {
    type Error = T::Error;

    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send
    {
        self.as_ref().send_http(request)
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: HttpClient> HttpClient for Arc<T> {
    type Error = T::Error;

    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> {
        self.as_ref().send_http(request)
    }
}

#[cfg(all(feature = "streaming", feature = "reqwest-client"))]
impl HttpClientExt for reqwest::Client {
    async fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<ByteStream>, Self::Error> {
        // Convert http::Request to reqwest::Request
        let (parts, body) = request.into_parts();

        let mut req = self.request(parts.method, parts.uri.to_string()).body(body);

        // Copy headers
        for (name, value) in parts.headers.iter() {
            req = req.header(name.as_str(), value.as_bytes());
        }

        // Send request and get streaming response
        let resp = req.send().await?;

        // Convert reqwest::Response to http::Response with ByteStream
        let mut builder = http::Response::builder().status(resp.status());

        // Copy headers
        for (name, value) in resp.headers().iter() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }

        // Convert bytes_stream to ByteStream
        use futures::StreamExt;
        let stream = resp.bytes_stream().map(|result| {
            result.map_err(|e| StreamError::transport(e))
        });
        let byte_stream = ByteStream::new(stream);

        Ok(builder.body(byte_stream).expect("Failed to build response"))
    }

    async fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> Result<http::Response<ByteStream>, Self::Error>
    where
        S: n0_future::Stream<Item = bytes::Bytes> + Send + 'static,
    {
        // Convert stream to reqwest::Body
        use futures::StreamExt;
        let ok_stream = body.map(Ok::<_, Self::Error>);
        let reqwest_body = reqwest::Body::wrap_stream(ok_stream);

        let mut req = self
            .request(parts.method, parts.uri.to_string())
            .body(reqwest_body);

        // Copy headers
        for (name, value) in parts.headers.iter() {
            req = req.header(name.as_str(), value.as_bytes());
        }

        // Send and convert response
        let resp = req.send().await?;

        let mut builder = http::Response::builder().status(resp.status());

        for (name, value) in resp.headers().iter() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }

        let stream = resp.bytes_stream().map(|result| {
            result.map_err(|e| StreamError::transport(e))
        });
        let byte_stream = ByteStream::new(stream);

        Ok(builder.body(byte_stream).expect("Failed to build response"))
    }
}
