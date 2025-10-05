//! Minimal HTTP client abstraction shared across crates.

use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

/// HTTP client trait for sending raw HTTP requests.
pub trait HttpClient {
    /// Error type returned by the HTTP client
    type Error: std::error::Error + Display + Send + Sync + 'static;
    /// Send an HTTP request and return the response.
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send;
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

impl<T: HttpClient> HttpClient for Arc<T> {
    type Error = T::Error;

    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send
    {
        self.as_ref().send_http(request)
    }
}
