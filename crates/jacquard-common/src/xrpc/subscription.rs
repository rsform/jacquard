//! WebSocket subscription support for XRPC
//!
//! This module defines traits and types for typed WebSocket subscriptions,
//! mirroring the request/response pattern used for HTTP XRPC endpoints.

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::future::Future;
use url::Url;

use crate::websocket::{WebSocketClient, WebSocketConnection};
use crate::{CowStr, IntoStatic};

/// Encoding format for subscription messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageEncoding {
    /// JSON text frames
    Json,
    /// DAG-CBOR binary frames
    DagCbor,
}

/// XRPC subscription stream response trait
///
/// Analogous to `XrpcResp` but for WebSocket subscriptions.
/// Defines the message and error types for a subscription stream.
///
/// This trait is implemented on a marker struct to keep it lifetime-free
/// while using GATs for the message/error types.
pub trait SubscriptionResp {
    /// The NSID for this subscription
    const NSID: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Message union type
    type Message<'de>: Deserialize<'de> + IntoStatic;

    /// Error union type
    type Error<'de>: Error + Deserialize<'de> + IntoStatic;
}

/// XRPC subscription (WebSocket)
///
/// This trait is analogous to `XrpcRequest` but for WebSocket subscriptions.
/// It defines the NSID and associated stream response type.
///
/// The trait is implemented on the subscription parameters type.
pub trait XrpcSubscription: Serialize {
    /// The NSID for this XRPC subscription
    const NSID: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Stream response type (marker struct)
    type Stream: SubscriptionResp;

    /// Encode query params for WebSocket URL
    ///
    /// Default implementation uses serde_html_form to encode the struct as query parameters.
    fn query_params(&self) -> Vec<(String, String)> {
        // Default: use serde_html_form to encode self
        serde_html_form::to_string(self)
            .ok()
            .map(|s| {
                s.split('&')
                    .filter_map(|pair| {
                        let mut parts = pair.splitn(2, '=');
                        Some((parts.next()?.to_string(), parts.next()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// XRPC subscription endpoint trait (server-side)
///
/// Analogous to `XrpcEndpoint` but for WebSocket subscriptions.
/// Defines the fully-qualified path and associated parameter/stream types.
///
/// This exists primarily for server-side frameworks (like Axum) to extract
/// typed subscription parameters without lifetime issues.
pub trait SubscriptionEndpoint {
    /// Fully-qualified path ('/xrpc/[nsid]') where this subscription endpoint lives
    const PATH: &'static str;

    /// Message encoding (JSON or DAG-CBOR)
    const ENCODING: MessageEncoding;

    /// Subscription parameters type
    type Params<'de>: XrpcSubscription + Deserialize<'de> + IntoStatic;

    /// Stream response type
    type Stream: SubscriptionResp;
}

/// Per-subscription options for WebSocket subscriptions.
#[derive(Debug, Default, Clone)]
pub struct SubscriptionOptions<'a> {
    /// Extra headers to attach to this subscription (e.g., Authorization).
    pub headers: Vec<(CowStr<'a>, CowStr<'a>)>,
}

impl IntoStatic for SubscriptionOptions<'_> {
    type Output = SubscriptionOptions<'static>;

    fn into_static(self) -> Self::Output {
        SubscriptionOptions {
            headers: self
                .headers
                .into_iter()
                .map(|(k, v)| (k.into_static(), v.into_static()))
                .collect(),
        }
    }
}

/// Extension for stateless subscription calls on any `WebSocketClient`.
///
/// Provides a builder pattern for establishing WebSocket subscriptions with custom options.
pub trait SubscriptionExt: WebSocketClient {
    /// Start building a subscription call for the given base URL.
    fn subscription<'a>(&'a self, base: Url) -> SubscriptionCall<'a, Self>
    where
        Self: Sized,
    {
        SubscriptionCall {
            client: self,
            base,
            opts: SubscriptionOptions::default(),
        }
    }
}

impl<T: WebSocketClient> SubscriptionExt for T {}

/// Stateless subscription call builder.
///
/// Provides methods for adding headers and establishing typed subscriptions.
pub struct SubscriptionCall<'a, C: WebSocketClient> {
    pub(crate) client: &'a C,
    pub(crate) base: Url,
    pub(crate) opts: SubscriptionOptions<'a>,
}

impl<'a, C: WebSocketClient> SubscriptionCall<'a, C> {
    /// Add an extra header.
    pub fn header(mut self, name: impl Into<CowStr<'a>>, value: impl Into<CowStr<'a>>) -> Self {
        self.opts.headers.push((name.into(), value.into()));
        self
    }

    /// Replace the builder's options entirely.
    pub fn with_options(mut self, opts: SubscriptionOptions<'a>) -> Self {
        self.opts = opts;
        self
    }

    /// Subscribe to the given XRPC subscription endpoint.
    ///
    /// Builds a WebSocket URL from the base, appends the NSID path,
    /// encodes query parameters from the subscription type, and connects.
    pub async fn subscribe<Sub>(self, params: &Sub) -> Result<WebSocketConnection, C::Error>
    where
        Sub: XrpcSubscription,
    {
        let mut url = self.base.clone();
        let mut path = url.path().trim_end_matches('/').to_owned();
        path.push_str("/xrpc/");
        path.push_str(Sub::NSID);
        url.set_path(&path);

        let query_params = params.query_params();
        if !query_params.is_empty() {
            let qs = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url.set_query(Some(&qs));
        } else {
            url.set_query(None);
        }

        self.client
            .connect_with_headers(url, self.opts.headers)
            .await
    }
}

/// Stateful subscription client trait.
///
/// Analogous to `XrpcClient` but for WebSocket subscriptions.
/// Provides a stateful interface for subscribing with configured base URI and options.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait SubscriptionClient: WebSocketClient {
    /// Get the base URI for the client.
    fn base_uri(&self) -> Url;

    /// Get the subscription options for the client.
    fn subscription_opts(&self) -> impl Future<Output = SubscriptionOptions<'_>> {
        async { SubscriptionOptions::default() }
    }

    /// Subscribe to an XRPC subscription endpoint using the client's base URI and options.
    #[cfg(not(target_arch = "wasm32"))]
    fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>>
    where
        Sub: XrpcSubscription + Send + Sync,
        Self: Sync;

    /// Subscribe to an XRPC subscription endpoint using the client's base URI and options.
    #[cfg(target_arch = "wasm32")]
    fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>>
    where
        Sub: XrpcSubscription + Send + Sync;

    /// Subscribe with custom options.
    #[cfg(not(target_arch = "wasm32"))]
    fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>>
    where
        Sub: XrpcSubscription + Send + Sync,
        Self: Sync;

    /// Subscribe with custom options.
    #[cfg(target_arch = "wasm32")]
    fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>>
    where
        Sub: XrpcSubscription + Send + Sync;
}

/// Simple stateless subscription client wrapping a WebSocketClient.
///
/// Analogous to a basic HTTP client but for WebSocket subscriptions.
/// Does not manage sessions or authentication - useful for public subscriptions
/// or when you want to handle auth manually via headers.
pub struct BasicSubscriptionClient<W: WebSocketClient> {
    client: W,
    base_uri: Url,
    opts: SubscriptionOptions<'static>,
}

impl<W: WebSocketClient> BasicSubscriptionClient<W> {
    /// Create a new basic subscription client with the given WebSocket client and base URI.
    pub fn new(client: W, base_uri: Url) -> Self {
        Self {
            client,
            base_uri,
            opts: SubscriptionOptions::default(),
        }
    }

    /// Create with default options.
    pub fn with_options(mut self, opts: SubscriptionOptions<'_>) -> Self {
        self.opts = opts.into_static();
        self
    }

    /// Get a reference to the inner WebSocket client.
    pub fn inner(&self) -> &W {
        &self.client
    }
}

impl<W: WebSocketClient> WebSocketClient for BasicSubscriptionClient<W> {
    type Error = W::Error;

    async fn connect(&self, url: Url) -> Result<WebSocketConnection, Self::Error> {
        self.client.connect(url).await
    }

    async fn connect_with_headers(
        &self,
        url: Url,
        headers: Vec<(CowStr<'_>, CowStr<'_>)>,
    ) -> Result<WebSocketConnection, Self::Error> {
        self.client.connect_with_headers(url, headers).await
    }
}

impl<W: WebSocketClient> SubscriptionClient for BasicSubscriptionClient<W> {
    fn base_uri(&self) -> Url {
        self.base_uri.clone()
    }

    async fn subscription_opts(&self) -> SubscriptionOptions<'_> {
        self.opts.clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> Result<WebSocketConnection, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync,
        Self: Sync,
    {
        let opts = self.subscription_opts().await;
        self.subscribe_with_opts(params, opts).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> Result<WebSocketConnection, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync,
    {
        let opts = self.subscription_opts().await;
        self.subscribe_with_opts(params, opts).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> Result<WebSocketConnection, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync,
        Self: Sync,
    {
        let base = self.base_uri();
        self.subscription(base)
            .with_options(opts)
            .subscribe(params)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: SubscriptionOptions<'_>,
    ) -> Result<WebSocketConnection, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync,
    {
        let base = self.base_uri();
        self.subscription(base)
            .with_options(opts)
            .subscribe(params)
            .await
    }
}

/// Type alias for a basic subscription client using the default TungsteniteClient.
///
/// Provides a simple, stateless WebSocket subscription client without session management.
/// Useful for public subscriptions or when handling authentication manually.
///
/// # Example
///
/// ```no_run
/// # use jacquard_common::xrpc::{TungsteniteSubscriptionClient, SubscriptionClient};
/// # use url::Url;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let base = Url::parse("wss://bsky.network")?;
/// let client = TungsteniteSubscriptionClient::from_base_uri(base);
/// // let conn = client.subscribe(&params).await?;
/// # Ok(())
/// # }
/// ```
pub type TungsteniteSubscriptionClient =
    BasicSubscriptionClient<crate::websocket::tungstenite_client::TungsteniteClient>;

impl TungsteniteSubscriptionClient {
    /// Create a new Tungstenite-backed subscription client with the given base URI.
    pub fn from_base_uri(base_uri: Url) -> Self {
        let client = crate::websocket::tungstenite_client::TungsteniteClient::new();
        BasicSubscriptionClient::new(client, base_uri)
    }
}
