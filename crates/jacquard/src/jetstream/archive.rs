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
use jacquard_common::error::{ClientError, ClientErrorKind, HttpError, XrpcResult};
use jacquard_common::http_client::{HttpClient, HttpClientExt};
use jacquard_common::stream::{ByteStream, StreamError};
use jacquard_common::xrpc::streaming::{
    XrpcProcedureSend, XrpcProcedureStream, XrpcResponseStream, XrpcStreamResp,
};
use jacquard_common::xrpc::{
    CallOptions, GenericXrpcError, Response, StreamingResponse, XrpcClient, XrpcExt, XrpcRequest,
    XrpcResponse, XrpcStreamingClient, normalize_base_uri,
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

fn map_client_error<E>(error: ClientError) -> JetstreamError<E>
where
    E: core::error::Error + Send + Sync + 'static,
{
    let message = error.to_string();
    let status = error.status();
    let headers = error.headers().cloned();
    let body = error
        .source_err()
        .and_then(|source| source.downcast_ref::<HttpError>())
        .and_then(|http_error| http_error.body.clone());

    if matches!(error.kind(), ClientErrorKind::Auth(_))
        || status == Some(http::StatusCode::UNAUTHORIZED)
    {
        return JetstreamError::InvalidBearerCredential;
    }

    match status {
        Some(http::StatusCode::TOO_MANY_REQUESTS) => {
            let retry_after = headers
                .as_ref()
                .and_then(|headers| headers.get(http::header::RETRY_AFTER))
                .and_then(|value| value.to_str().ok())
                .and_then(parse_retry_after);
            JetstreamError::ByteLimitExceeded { retry_after }
        }
        Some(http::StatusCode::NOT_FOUND) => {
            let error_body = body
                .as_deref()
                .and_then(|body| serde_json::from_slice::<GenericXrpcError>(body).ok());
            JetstreamError::NotFound {
                error: error_body
                    .map(|body| body.error)
                    .unwrap_or_else(|| SmolStr::new_static("NotFound")),
            }
        }
        Some(status) => JetstreamError::UnexpectedStatus {
            status,
            body: body
                .as_deref()
                .and_then(|body| serde_json::from_slice::<GenericXrpcError>(body).ok())
                .map(|body| match body.message {
                    Some(message) => format!("{}: {message}", body.error),
                    None => body.error.to_string(),
                })
                .unwrap_or_else(|| {
                    body.as_ref()
                        .map(|body| format!("{} bytes", body.len()))
                        .unwrap_or_else(|| "empty response".to_owned())
                })
                .into(),
        },
        None => {
            let source = error
                .into_source()
                .and_then(|source| source.downcast::<E>().ok())
                .map(|source| *source);
            match source {
                Some(source) => JetstreamError::Transport(source),
                None => JetstreamError::Decode(message.into()),
            }
        }
    }
}

fn map_response_error<E, R>(response: Response<R>) -> JetstreamError<E>
where
    E: core::error::Error + Send + Sync + 'static,
    R: jacquard_common::xrpc::XrpcResp,
{
    let status = response.status();
    let headers = response.headers();
    let body = response.buffer();

    match status {
        http::StatusCode::UNAUTHORIZED => JetstreamError::InvalidBearerCredential,
        http::StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = headers
                .get(http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_retry_after);
            JetstreamError::ByteLimitExceeded { retry_after }
        }
        http::StatusCode::NOT_FOUND => {
            let error_body = serde_json::from_slice::<GenericXrpcError>(body).ok();
            JetstreamError::NotFound {
                error: error_body
                    .map(|body| body.error)
                    .unwrap_or_else(|| SmolStr::new_static("NotFound")),
            }
        }
        status => JetstreamError::UnexpectedStatus {
            status,
            body: serde_json::from_slice::<GenericXrpcError>(body)
                .ok()
                .map(|body| match body.message {
                    Some(message) => format!("{}: {message}", body.error),
                    None => body.error.to_string(),
                })
                .unwrap_or_else(|| format!("{} bytes", body.len()))
                .into(),
        },
    }
}

/// Errors surfaced by the Jetstream v2 HTTP transport.
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

    /// Send one typed Jetstream v2 request through the shared XRPC response path.
    async fn send_jetstream<R>(
        &self,
        request: &R,
        extra_headers: &[(http::HeaderName, http::HeaderValue)],
    ) -> Result<XrpcResponse<R>, JetstreamError<C::Error>>
    where
        R: XrpcRequest + serde::Serialize,
        R::Response: Send + Sync,
    {
        let mut opts = self.call_options().await;
        opts.extra_headers.extend_from_slice(extra_headers);
        let base = self.base.read().await;
        self.http
            .xrpc(base.borrow())
            .with_options(opts)
            .send(request)
            .await
            .map_err(map_client_error)
            .and_then(|response| {
                if response.status().is_success() {
                    Ok(response)
                } else {
                    Err(map_response_error(response))
                }
            })
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
            body: response.buffer().clone(),
        })
    }

    /// `GET network.bsky.jetstream.getSegment`: the raw sealed `.jss`
    /// bytes for one segment.
    pub async fn get_segment(&self, name: &str) -> Result<Bytes, JetstreamError<C::Error>> {
        let params = GetSegment::<DefaultStr> { name: name.into() };
        Ok(self.send_jetstream(&params, &[]).await?.buffer().clone())
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
        Ok(self.send_jetstream(&params, &[]).await?.buffer().clone())
    }

    /// `GET network.bsky.jetstream.getZstdDictionary`: the raw zstd
    /// dictionary bytes. Omit `id` for the server's current dictionary.
    pub async fn get_zstd_dictionary(
        &self,
        id: Option<i64>,
    ) -> Result<Bytes, JetstreamError<C::Error>> {
        let params = GetZstdDictionary { id };
        Ok(self.send_jetstream(&params, &[]).await?.buffer().clone())
    }

    /// `GET network.bsky.jetstream.getSegment` with a byte offset via HTTP
    /// `Range`. Callers can use this to resume a partial download.
    pub async fn get_segment_range(
        &self,
        name: &str,
        offset: u64,
    ) -> Result<XrpcResponse<GetSegment<DefaultStr>>, JetstreamError<C::Error>> {
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
    async fn archive_preserves_range_response_metadata() {
        let server = MockServer::start().await;
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/xrpc/network.bsky.jetstream.getSegment"))
                    .and(header("range", "bytes=7-"))
                    .respond_with(
                        ResponseTemplate::new(206)
                            .insert_header("content-range", "bytes 7-9/10")
                            .set_body_bytes(vec![7, 8, 9]),
                    ),
            )
            .await;
        let client = client_for(&server, None);

        let response = client.get_segment_range("segment.jss", 7).await.unwrap();
        assert_eq!(response.status(), http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get("content-range").unwrap(),
            "bytes 7-9/10"
        );
        assert_eq!(response.buffer().as_ref(), &[7, 8, 9]);
        server.verify().await;
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
