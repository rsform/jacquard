//! Fixture-only HTTP transport for e2e scenarios.
//!
//! Every native identity/OAuth/XRPC path goes through this wrapper. Before
//! each send it validates the request URI scheme, logical hostname, and port
//! against the fixture allowlist and rejects raw IPs, PLC/public hosts,
//! unapproved ports, and redirects.

use std::collections::BTreeSet;

use http::{Request as HttpRequest, Response as HttpResponse};
use jacquard_common::http_client::HttpClient;

/// Public PLC directory hosts that must never be contacted by a fixture run.
const PLC_HOSTS: [&str; 2] = ["plc.directory", "directory.dev"];

/// A host allowlist entry: logical hostname, approved port, and the single
/// scheme permitted for that host. Fixture identity and app hosts are
/// HTTPS-only (per-run CA); the provider PDS is plain HTTP because it is
/// addressed directly by its Docker bridge IP.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AllowedHost {
    pub host: String,
    pub port: u16,
    pub scheme: &'static str,
}

/// Errors surfaced by the fixture transport. Deliberately free of request
/// bodies and header values so diagnostics cannot leak credentials.
#[derive(Debug, thiserror::Error)]
pub enum FixtureTransportError {
    #[error("fixture transport rejected non-HTTPS target {scheme}://{host}:{port}")]
    NonHttpsTarget {
        scheme: String,
        host: String,
        port: u16,
    },
    #[error("fixture transport rejected non-allowlisted target https://{host}:{port}")]
    DisallowedTarget { host: String, port: u16 },
    #[error("fixture transport rejected raw-IP target {target}")]
    RawTarget { target: String },
    #[error("fixture transport rejected PLC resolution attempt for {host}")]
    PlcTarget { host: String },
    #[error("fixture transport rejected redirect to {location}")]
    DisallowedRedirect { location: String },
    #[error("fixture transport could not build request: {0}")]
    InvalidRequest(String),
    #[error("http request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
}

/// The allowlisted set of hosts this run may contact.
#[derive(Debug, Clone)]
pub struct TransportAllowlist {
    hosts: BTreeSet<AllowedHost>,
}

impl TransportAllowlist {
    /// Build an allowlist from explicit host/port pairs. Every entry must be
    /// reached over HTTPS on its approved port.
    pub fn new(hosts: impl IntoIterator<Item = AllowedHost>) -> Self {
        Self {
            hosts: hosts.into_iter().collect(),
        }
    }

    fn check(&self, uri: &http::Uri) -> Result<(), FixtureTransportError> {
        let scheme = uri.scheme_str().unwrap_or_default();
        let host = uri.host().unwrap_or_default();
        let port = uri.port_u16().unwrap_or(match scheme {
            "https" => 443,
            "http" => 80,
            _ => 0,
        });
        let scheme: &'static str = match scheme {
            "https" => "https",
            "http" => "http",
            other => {
                return Err(FixtureTransportError::NonHttpsTarget {
                    scheme: other.to_string(),
                    host: host.to_string(),
                    port,
                });
            }
        };
        let entry = AllowedHost {
            host: host.to_string(),
            port,
            scheme,
        };
        if !self.hosts.contains(&entry) {
            if host.parse::<std::net::IpAddr>().is_ok() {
                return Err(FixtureTransportError::RawTarget {
                    target: host.to_string(),
                });
            }
            if PLC_HOSTS.contains(&host) {
                return Err(FixtureTransportError::PlcTarget {
                    host: host.to_string(),
                });
            }
            if scheme == "https" || scheme == "http" {
                return Err(FixtureTransportError::DisallowedTarget {
                    host: host.to_string(),
                    port,
                });
            }
            return Err(FixtureTransportError::NonHttpsTarget {
                scheme: scheme.to_string(),
                host: host.to_string(),
                port,
            });
        }
        Ok(())
    }
}

/// A `reqwest::Client` wrapped with fixture allowlist enforcement.
///
/// The underlying client must already be configured with per-host `resolve`
/// mappings, the per-run CA, and redirect policy `none`. Redirect responses
/// are rejected rather than followed.
#[derive(Debug, Clone)]
pub struct FixtureTransport {
    client: reqwest::Client,
    allowlist: TransportAllowlist,
}

impl FixtureTransport {
    /// Wrap a pre-configured client with the per-run allowlist.
    pub fn new(client: reqwest::Client, allowlist: TransportAllowlist) -> Self {
        Self { client, allowlist }
    }
}

impl HttpClient for FixtureTransport {
    type Error = FixtureTransportError;

    async fn send_http(
        &self,
        request: HttpRequest<Vec<u8>>,
    ) -> Result<HttpResponse<Vec<u8>>, Self::Error> {
        let uri = request.uri().clone();
        self.allowlist.check(&uri)?;

        let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;
        let url = uri
            .to_string()
            .parse::<reqwest::Url>()
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;

        let (parts, body) = request.into_parts();
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &parts.headers {
            let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()) else {
                continue;
            };
            if let Ok(value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                headers.insert(name, value);
            }
        }

        let response = self
            .client
            .request(method, url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            return Err(FixtureTransportError::DisallowedRedirect { location });
        }

        let mut builder = HttpResponse::builder().status(status.as_u16());
        for (name, value) in response.headers() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }
        let bytes = response.bytes().await?;
        builder
            .body(bytes.to_vec())
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))
    }
}

#[cfg(feature = "e2e")]
impl jacquard_common::http_client::HttpClientExt for FixtureTransport {
    async fn send_http_streaming(
        &self,
        request: HttpRequest<Vec<u8>>,
    ) -> Result<HttpResponse<jacquard_common::stream::ByteStream>, Self::Error> {
        use jacquard_common::stream::{ByteStream, StreamError};
        use n0_future::TryStreamExt as _;
        let uri = request.uri().clone();
        self.allowlist.check(&uri)?;

        let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;
        let url = uri
            .to_string()
            .parse::<reqwest::Url>()
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;

        let (parts, body) = request.into_parts();
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &parts.headers {
            let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()) else {
                continue;
            };
            if let Ok(value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                headers.insert(name, value);
            }
        }

        let response = self
            .client
            .request(method, url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            return Err(FixtureTransportError::DisallowedRedirect { location });
        }

        let mut builder = HttpResponse::builder().status(status.as_u16());
        for (name, value) in response.headers() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }
        let stream = response
            .bytes_stream()
            .map_err(|e| StreamError::transport(FixtureTransportError::Reqwest(e)));
        let response = builder
            .body(ByteStream::new(Box::pin(stream)))
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;
        Ok(response)
    }

    async fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> Result<HttpResponse<jacquard_common::stream::ByteStream>, Self::Error>
    where
        S: n0_future::Stream<Item = Result<jacquard::deps::bytes::Bytes, jacquard_common::stream::StreamError>>
            + Send
            + 'static,
    {
        use jacquard_common::stream::{ByteStream, StreamError};
        use n0_future::TryStreamExt as _;
        let uri = parts.uri.clone();
        self.allowlist.check(&uri)?;

        let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;
        let url = uri
            .to_string()
            .parse::<reqwest::Url>()
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;

        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &parts.headers {
            let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()) else {
                continue;
            };
            if let Ok(value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                headers.insert(name, value);
            }
        }

        let body = reqwest::Body::wrap_stream(
            body.map_err(|e| FixtureTransportError::InvalidRequest(e.to_string())),
        );
        let response = self
            .client
            .request(method, url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            return Err(FixtureTransportError::DisallowedRedirect { location });
        }

        let mut builder = HttpResponse::builder().status(status.as_u16());
        for (name, value) in response.headers() {
            builder = builder.header(name.as_str(), value.as_bytes());
        }
        let stream = response
            .bytes_stream()
            .map_err(|e| StreamError::transport(FixtureTransportError::Reqwest(e)));
        let response = builder
            .body(ByteStream::new(Box::pin(stream)))
            .map_err(|e| FixtureTransportError::InvalidRequest(e.to_string()))?;
        Ok(response)
    }
}
