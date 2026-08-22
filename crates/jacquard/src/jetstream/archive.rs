//! Backfill transport for Jetstream v2: the HTTP boundary for
//! `network.bsky.jetstream` archive endpoints.
//!
//! Wraps any [`HttpClient`] and injects the API key
//! (`Authorization: Bearer <key>`) on every request

use bytes::Bytes;
use jacquard_api::network_bsky::jetstream::get_block::GetBlock;
use jacquard_api::network_bsky::jetstream::get_segment::GetSegment;
use jacquard_api::network_bsky::jetstream::get_zstd_dictionary::GetZstdDictionary;
use jacquard_api::network_bsky::jetstream::plan_snapshot::{PlanSnapshot, PlanSnapshotOutput};
use jacquard_common::AuthorizationToken;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::error::XrpcResult;
use jacquard_common::http_client::{HttpClient, HttpClientExt};
use jacquard_common::stream::{ByteStream, StreamError};
use jacquard_common::xrpc::streaming::{
    XrpcProcedureSend, XrpcProcedureStream, XrpcResponseStream, XrpcStreamResp,
};
use jacquard_common::xrpc::{
    CallOptions, GenericXrpcError, StreamingResponse, XrpcClient, XrpcExt, XrpcRequest,
    XrpcResponse, XrpcStreamingClient, build_http_request, normalize_base_uri,
};
use smol_str::{SmolStr, format_smolstr};

type DefaultStr = jacquard_common::DefaultStr;

/// Parse `Retry-After` per RFC 9110: delta-seconds or HTTP-date
/// (normalized to whole seconds from now).
fn parse_retry_after(value: &str) -> Option<u64> {
    if let Ok(secs) = value.parse::<u64>() {
        return Some(secs);
    }
    let at = httpdate::parse_http_date(value.trim()).ok()?;
    let now = std::time::SystemTime::now();
    Some(at.duration_since(now).map(|d| d.as_secs()).unwrap_or(0))
}

/// Errors surfaced by the jetstream v2 http transport.
#[derive(Debug)]
pub enum JetstreamError<E> {
    /// Underlying HTTP transport failure.
    Transport(E),
    /// 401: the API key is missing, malformed, or revoked
    /// (`{"error":"invalid bearer credential"}`).
    InvalidBearerCredential,
    /// 429: the metered byte budget is exhausted.
    ByteLimitExceeded {
        /// The server's `Retry-After` in seconds when present.
        retry_after: Option<u64>,
    },
    /// 404 carrying a generated error name (`SegmentNotFound`,
    /// `BlockNotFound`, `DictionaryNotFound`).
    NotFound {
        /// The generated error name from the response body.
        error: SmolStr,
    },
    /// Any other non-success status with its bounded body.
    UnexpectedStatus {
        /// The HTTP status.
        status: http::StatusCode,
        /// The response body, bounded.
        body: SmolStr,
    },
    /// A 2xx body failed to decode into the generated output type.
    Decode(SmolStr),
}

impl<E: core::fmt::Display> core::fmt::Display for JetstreamError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "archive transport error: {e}"),
            Self::InvalidBearerCredential => {
                write!(f, "invalid bearer credential (check the archive API key)")
            }
            Self::ByteLimitExceeded { retry_after } => match retry_after {
                Some(secs) => write!(f, "byte limit exceeded; retry after {secs}s"),
                None => write!(f, "byte limit exceeded"),
            },
            Self::NotFound { error } => write!(f, "{error}"),
            Self::UnexpectedStatus { status, body } => write!(f, "HTTP {status}: {body}"),
            Self::Decode(msg) => write!(f, "failed to decode archive response: {msg}"),
        }
    }
}

impl<E: core::fmt::Display + core::fmt::Debug> std::error::Error for JetstreamError<E> {}

/// Jetstream backfill/archive client over any [`HttpClient`].
pub struct JetstreamClient<C: HttpClient> {
    http: C,
    base: tokio::sync::RwLock<Uri<String>>,
    options: tokio::sync::RwLock<CallOptions>,
}

impl<C: HttpClient> JetstreamClient<C> {
    /// Create a client for a jetstream instance at `base` with an optional
    /// API key. Public instances require the key; self-hosted ones may not.
    pub fn new(http: C, base: Uri<String>, api_key: Option<SmolStr>) -> Self {
        let mut options = CallOptions::default();
        if let Some(key) = api_key {
            options.auth = Some(AuthorizationToken::Bearer(key));
        }
        Self {
            http,
            base: tokio::sync::RwLock::new(normalize_base_uri(base)),
            options: tokio::sync::RwLock::new(options),
        }
    }

    async fn call_options(&self) -> CallOptions {
        self.options.read().await.clone()
    }

    /// Send one typed jetstream v2 (mostly backfill-oriented) request and process the raw response,
    /// preserving headers the standard XRPC path would drop.
    async fn send_jetstream<R>(
        &self,
        request: &R,
        extra_headers: &[(http::HeaderName, http::HeaderValue)],
    ) -> Result<http::Response<Vec<u8>>, JetstreamError<C::Error>>
    where
        R: XrpcRequest + serde::Serialize,
    {
        let mut opts = self.call_options().await;
        opts.extra_headers.extend_from_slice(extra_headers);
        let base_guard = self.base.read().await;
        let base = base_guard.borrow();
        let http_request = build_http_request(&base, request, &opts)
            .map_err(|e| JetstreamError::Decode(format_smolstr!("failed to build request: {e}")))?;

        let response = self
            .http
            .send_http(http_request)
            .await
            .map_err(JetstreamError::Transport)?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let (parts, body) = response.into_parts();
        let error_body = serde_json::from_slice::<GenericXrpcError>(&body)
            .ok()
            .map(|mut e| {
                e.nsid = R::NSID;
                e.method = R::METHOD.as_str();
                e.http_status = status;
                e
            });

        match status.as_u16() {
            401 => Err(JetstreamError::InvalidBearerCredential),
            404 => Err(JetstreamError::NotFound {
                error: error_body
                    .map(|b| b.error)
                    .unwrap_or_else(|| SmolStr::new_static("NotFound")),
            }),
            429 => {
                let retry_after = parts
                    .headers
                    .get(http::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);
                Err(JetstreamError::ByteLimitExceeded { retry_after })
            }
            _ => Err(JetstreamError::UnexpectedStatus {
                status,
                body: error_body
                    .map(|b| match b.message {
                        Some(message) => format!("{}: {message}", b.error),
                        None => b.error.to_string(),
                    })
                    .unwrap_or_else(|| format!("{} bytes", body.len()))
                    .into(),
            }),
        }
    }

    /// `POST network.bsky.jetstream.planSnapshot`.
    ///
    /// Fetch one `planSnapshot` page.
    pub async fn plan_snapshot_page(
        &self,
        params: &PlanSnapshot<DefaultStr>,
    ) -> Result<PlanSnapshotPage, JetstreamError<C::Error>> {
        let response = self.send_jetstream(params, &[]).await?;
        Ok(PlanSnapshotPage {
            body: response.into_body().into(),
        })
    }

    /// `GET network.bsky.jetstream.getSegment`: the raw sealed `.jss`
    /// bytes for one segment.
    pub async fn get_segment(&self, name: &str) -> Result<Bytes, JetstreamError<C::Error>> {
        let params = GetSegment::<DefaultStr> { name: name.into() };
        Ok(self.send_jetstream(&params, &[]).await?.into_body().into())
    }

    /// `GET network.bsky.jetstream.getBlock`: one block's bare zstd frame
    /// (no length prefix) within a sealed segment.
    pub async fn get_block(
        &self,
        segment: &str,
        block_index: i64,
    ) -> Result<Bytes, JetstreamError<C::Error>> {
        let params = GetBlock::<DefaultStr> {
            segment: segment.into(),
            block_index,
        };
        Ok(self.send_jetstream(&params, &[]).await?.into_body().into())
    }

    /// `GET network.bsky.jetstream.getZstdDictionary`: the raw zstd
    /// dictionary bytes. Omit `id` for the server's current dictionary.
    pub async fn get_zstd_dictionary(
        &self,
        id: Option<i64>,
    ) -> Result<Bytes, JetstreamError<C::Error>> {
        let params = GetZstdDictionary { id };
        Ok(self.send_jetstream(&params, &[]).await?.into_body().into())
    }

    /// `GET network.bsky.jetstream.getSegment` with a byte offset via HTTP
    /// `Range`. Callers can use this to resume a partial download.
    pub async fn get_segment_range(
        &self,
        name: &str,
        offset: u64,
    ) -> Result<http::Response<Vec<u8>>, JetstreamError<C::Error>> {
        let params = GetSegment::<DefaultStr> { name: name.into() };
        let range = http::HeaderValue::from_str(&format!("bytes={offset}-"))
            .map_err(|e| JetstreamError::Decode(format_smolstr!("invalid range offset: {e}")))?;
        self.send_jetstream(&params, &[(http::header::RANGE, range)])
            .await
    }
}

impl<C: HttpClient + Sync> HttpClient for JetstreamClient<C> {
    type Error = C::Error;

    async fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, Self::Error> {
        self.http.send_http(request).await
    }
}

/// One fetched `planSnapshot` page, holding the raw response bytes so
/// decoding can borrow from them.
pub struct PlanSnapshotPage {
    body: Bytes,
}

impl PlanSnapshotPage {
    /// Decode the page into the chosen string backing.
    pub fn parse<'de, S>(&'de self) -> Result<PlanSnapshotOutput<S>, PlanDecodeError>
    where
        S: jacquard_common::BosStr + serde::Deserialize<'de>,
    {
        serde_json::from_slice(&self.body)
            .map_err(|e| PlanDecodeError(format_smolstr!("planSnapshot output: {e}")))
    }
}

/// A `planSnapshot` page failed to decode.
#[derive(Debug)]
pub struct PlanDecodeError(pub SmolStr);

impl core::fmt::Display for PlanDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PlanDecodeError {}

impl<C: HttpClient + Sync> XrpcClient for JetstreamClient<C> {
    fn base_uri(&self) -> impl Future<Output = Uri<String>> {
        async { self.base.read().await.clone() }
    }

    fn set_base_uri(&self, uri: Uri<String>) -> impl Future<Output = ()> {
        async move {
            *self.base.write().await = normalize_base_uri(uri);
        }
    }

    fn opts(&self) -> impl Future<Output = CallOptions> {
        async { self.options.read().await.clone() }
    }

    fn set_opts(&self, opts: CallOptions) -> impl Future<Output = ()> {
        async move {
            *self.options.write().await = opts;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            let opts = self.call_options().await;
            let base = self.base.read().await;
            self.http
                .xrpc(base.borrow())
                .with_options(opts)
                .send(&request)
                .await
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            let base = self.base.read().await;
            self.http
                .xrpc(base.borrow())
                .with_options(opts)
                .send(&request)
                .await
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        async move {
            let base = self.base.read().await;
            self.http
                .xrpc(base.borrow())
                .with_options(opts)
                .send(&request)
                .await
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        async move {
            let opts = self.call_options().await;
            let base = self.base.read().await;
            self.http
                .xrpc(base.borrow())
                .with_options(opts)
                .send(&request)
                .await
        }
    }
}

impl<C> HttpClientExt for JetstreamClient<C>
where
    C: HttpClient + HttpClientExt + Sync,
{
    async fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<ByteStream>, Self::Error> {
        self.http.send_http_streaming(request).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> Result<http::Response<ByteStream>, Self::Error>
    where
        S: n0_future::Stream<Item = Result<Bytes, StreamError>> + Send + 'static,
    {
        self.http.send_http_bidirectional(parts, body).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> Result<http::Response<ByteStream>, Self::Error>
    where
        S: n0_future::Stream<Item = Result<Bytes, StreamError>> + 'static,
    {
        self.http.send_http_bidirectional(parts, body).await
    }
}

impl<C> XrpcStreamingClient for JetstreamClient<C>
where
    C: HttpClient + HttpClientExt + Sync,
{
    async fn download<R>(&self, request: R) -> Result<StreamingResponse, StreamError>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        use jacquard_common::xrpc::build_http_request;

        let base_uri = <Self as XrpcClient>::base_uri(self).await;
        let opts = self.call_options().await;
        let http_request = build_http_request(&base_uri.borrow(), &request, &opts)
            .map_err(|e| StreamError::protocol(e.to_string()))?;
        let response = self
            .http
            .send_http_streaming(http_request)
            .await
            .map_err(StreamError::transport)?;
        let (parts, body) = response.into_parts();
        Ok(StreamingResponse::new(parts, body))
    }

    async fn stream<Str, B>(
        &self,
        stream: XrpcProcedureSend<Str::Frame<B>>,
    ) -> core::result::Result<
        XrpcResponseStream<<<Str as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>>,
        StreamError,
    >
    where
        B: jacquard_common::BosStr + 'static,
        Str: XrpcProcedureStream + 'static,
        <<Str as XrpcProcedureStream>::Response as XrpcStreamResp>::Frame<B>: XrpcStreamResp,
    {
        use jacquard_common::StreamError;
        use n0_future::TryStreamExt as _;

        let base_uri = <Self as XrpcClient>::base_uri(self).await;
        let opts = self.call_options().await;

        let mut path = String::from(base_uri.as_str().trim_end_matches('/'));
        path.push_str("/xrpc/");
        path.push_str(<Str::Request as XrpcRequest>::NSID);

        let mut builder = http::Request::post(&path);
        if let Some(jacquard_common::AuthorizationToken::Bearer(t)) = &opts.auth {
            let hv = http::HeaderValue::from_str(&format!("Bearer {}", t.as_str()))
                .map_err(|e| StreamError::protocol(format!("invalid bearer key: {e}")))?;
            builder = builder.header(http::header::AUTHORIZATION, hv);
        }
        for (name, value) in &opts.extra_headers {
            builder = builder.header(name, value);
        }
        let (parts, _) = builder
            .body(())
            .map_err(|e| StreamError::protocol(e.to_string()))?
            .into_parts();

        let body_stream =
            jacquard_common::stream::ByteStream::new(Box::pin(stream.0.map_ok(|f| f.buffer)));
        let response = self
            .http
            .send_http_bidirectional(parts, body_stream.into_inner())
            .await
            .map_err(StreamError::transport)?;
        let (resp_parts, resp_body) = response.into_parts();
        Ok(XrpcResponseStream::from_typed_parts::<B>(
            resp_parts, resp_body,
        ))
    }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
    use super::*;
    use smol_str::SmolStr;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server: &MockServer, key: Option<&str>) -> JetstreamClient<reqwest::Client> {
        let base = jacquard_common::deps::fluent_uri::Uri::parse(server.uri().clone())
            .expect("server uri");
        JetstreamClient::new(reqwest::Client::new(), base, key.map(SmolStr::new))
    }

    #[tokio::test]
    async fn archive_requests_bearer_key_and_distinguishes_401() {
        let server = MockServer::start().await;
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getZstdDictionary"))
                    .and(header("authorization", "Bearer test-key"))
                    .respond_with(ResponseTemplate::new(404).set_body_bytes([]))
                    .up_to_n_times(1),
            )
            .await;
        let client = client_for(&server, Some("test-key"));

        client.get_zstd_dictionary(None).await.unwrap_err();
        server.verify().await;

        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getZstdDictionary"))
                    .respond_with(
                        ResponseTemplate::new(401)
                            .set_body_string(r#"{"error":"invalid bearer credential"}"#),
                    ),
            )
            .await;
        match client.get_zstd_dictionary(None).await {
            Err(JetstreamError::InvalidBearerCredential) => {}
            other => panic!("expected InvalidBearerCredential, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn archive_parses_retry_after_on_429() {
        let server = MockServer::start().await;
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getZstdDictionary"))
                    .respond_with(
                        ResponseTemplate::new(429)
                            .insert_header("retry-after", "7")
                            .set_body_bytes([]),
                    ),
            )
            .await;
        let client = client_for(&server, None);

        match client.get_zstd_dictionary(None).await {
            Err(JetstreamError::ByteLimitExceeded {
                retry_after: Some(7),
            }) => {}
            other => panic!("expected ByteLimitExceeded with Retry-After 7, got {other:?}"),
        }
    }
}
