//! WebSocket client abstraction

use crate::CowStr;
use crate::deps::fluent_uri::Uri;
use crate::stream::StreamError;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use bytes::Bytes;
use core::borrow::Borrow;
use core::fmt::{self, Display};
use core::future::Future;
use core::ops::Deref;
use core::pin::Pin;
use n0_future::Stream;

/// Maximum bytes of a rejected-upgrade response body retained in
/// [`WebSocketError::HandshakeRejected`]. Bodies beyond this are truncated.
const HANDSHAKE_BODY_LIMIT: usize = 64 * 1024;

/// Error type for WebSocket operations.
///
/// Wraps transport failures and surfaces rejected upgrade handshakes as a
/// structured variant, so protocol-level recovery information (e.g. the
/// retention floor in a Jetstream `CursorTooOld` rejection) is inspectable
/// instead of buried in an opaque transport error.
#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    /// The server rejected the WebSocket upgrade with an HTTP error status.
    #[error("websocket handshake rejected with HTTP {}", .status)]
    HandshakeRejected {
        /// The HTTP status of the rejection response.
        status: http::StatusCode,
        /// Response headers (bounded set).
        headers: Vec<(String, String)>,
        /// Bounded response body. Truncated at 64 KiB; may be empty if the
        /// platform could not capture it (wasm browser APIs expose no
        /// rejection body).
        body: Bytes,
    },
    /// Transport, TLS, or protocol failure.
    #[error(transparent)]
    Transport(Box<dyn core::error::Error + Send + Sync + 'static>),
}

impl WebSocketError {
    /// Wrap an underlying client error as a transport failure.
    pub fn transport<E>(err: E) -> Self
    where
        E: core::error::Error + Send + Sync + 'static,
    {
        Self::Transport(Box::new(err))
    }
}

impl From<tokio_tungstenite_wasm::Error> for WebSocketError {
    fn from(err: tokio_tungstenite_wasm::Error) -> Self {
        classify_tungstenite_error(err)
    }
}

/// Extract a handshake rejection from a tokio-tungstenite-wasm error, if
/// present. Other errors pass through unchanged.
pub(crate) fn classify_tungstenite_error(err: tokio_tungstenite_wasm::Error) -> WebSocketError {
    match err {
        tokio_tungstenite_wasm::Error::Http(response) => {
            let status = response.status();
            let headers = response
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    let v = core::str::from_utf8(v.as_bytes()).ok()?;
                    Some((k.as_str().to_string(), v.to_string()))
                })
                .collect();
            let mut body = response.into_body().unwrap_or_default();
            body.truncate(HANDSHAKE_BODY_LIMIT);
            WebSocketError::HandshakeRejected {
                status,
                headers,
                body: Bytes::from(body),
            }
        }
        other => WebSocketError::transport(other),
    }
}

/// UTF-8 validated bytes for WebSocket text messages
#[repr(transparent)]
#[derive(Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct WsText(Bytes);

impl WsText {
    /// Create from static string
    pub const fn from_static(s: &'static str) -> Self {
        Self(Bytes::from_static(s.as_bytes()))
    }

    /// Get as string slice
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }

    /// Create from bytes without validation (caller must ensure UTF-8)
    ///
    /// # Safety
    /// Bytes must be valid UTF-8
    pub unsafe fn from_bytes_unchecked(bytes: Bytes) -> Self {
        Self(bytes)
    }

    /// Convert into underlying bytes
    pub fn into_bytes(self) -> Bytes {
        self.0
    }
}

impl Deref for WsText {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for WsText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for WsText {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<Bytes> for WsText {
    fn as_ref(&self) -> &Bytes {
        &self.0
    }
}

impl Borrow<str> for WsText {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Display for WsText {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl From<String> for WsText {
    fn from(s: String) -> Self {
        Self(Bytes::from(s))
    }
}

impl From<&str> for WsText {
    fn from(s: &str) -> Self {
        Self(Bytes::copy_from_slice(s.as_bytes()))
    }
}

impl From<&String> for WsText {
    fn from(s: &String) -> Self {
        Self::from(s.as_str())
    }
}

impl TryFrom<Bytes> for WsText {
    type Error = core::str::Utf8Error;
    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        core::str::from_utf8(&bytes)?;
        Ok(Self(bytes))
    }
}

impl TryFrom<Vec<u8>> for WsText {
    type Error = core::str::Utf8Error;
    fn try_from(vec: Vec<u8>) -> Result<Self, Self::Error> {
        Self::try_from(Bytes::from(vec))
    }
}

impl From<WsText> for Bytes {
    fn from(t: WsText) -> Bytes {
        t.0
    }
}

impl Default for WsText {
    fn default() -> Self {
        Self(Bytes::new())
    }
}

/// WebSocket close code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum CloseCode {
    /// Normal closure
    Normal = 1000,
    /// Endpoint going away
    Away = 1001,
    /// Protocol error
    Protocol = 1002,
    /// Unsupported data
    Unsupported = 1003,
    /// Invalid frame payload data
    Invalid = 1007,
    /// Policy violation
    Policy = 1008,
    /// Message too big
    Size = 1009,
    /// Extension negotiation failure
    Extension = 1010,
    /// Unexpected condition
    Error = 1011,
    /// TLS handshake failure
    Tls = 1015,
    /// Other code
    Other(u16),
}

impl From<u16> for CloseCode {
    fn from(code: u16) -> Self {
        match code {
            1000 => CloseCode::Normal,
            1001 => CloseCode::Away,
            1002 => CloseCode::Protocol,
            1003 => CloseCode::Unsupported,
            1007 => CloseCode::Invalid,
            1008 => CloseCode::Policy,
            1009 => CloseCode::Size,
            1010 => CloseCode::Extension,
            1011 => CloseCode::Error,
            1015 => CloseCode::Tls,
            other => CloseCode::Other(other),
        }
    }
}

impl From<CloseCode> for u16 {
    fn from(code: CloseCode) -> u16 {
        match code {
            CloseCode::Normal => 1000,
            CloseCode::Away => 1001,
            CloseCode::Protocol => 1002,
            CloseCode::Unsupported => 1003,
            CloseCode::Invalid => 1007,
            CloseCode::Policy => 1008,
            CloseCode::Size => 1009,
            CloseCode::Extension => 1010,
            CloseCode::Error => 1011,
            CloseCode::Tls => 1015,
            CloseCode::Other(code) => code,
        }
    }
}

/// WebSocket close frame
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    /// Close code
    pub code: CloseCode,
    /// Close reason text
    pub reason: WsText,
}

impl CloseFrame {
    /// Create a new close frame
    pub fn new(code: CloseCode, reason: impl Into<WsText>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}

/// WebSocket message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMessage {
    /// Text message (UTF-8)
    Text(WsText),
    /// Binary message
    Binary(Bytes),
    /// Close frame
    Close(Option<CloseFrame>),
}

impl WsMessage {
    /// Check if this is a text message
    pub fn is_text(&self) -> bool {
        matches!(self, WsMessage::Text(_))
    }

    /// Check if this is a binary message
    pub fn is_binary(&self) -> bool {
        matches!(self, WsMessage::Binary(_))
    }

    /// Check if this is a close message
    pub fn is_close(&self) -> bool {
        matches!(self, WsMessage::Close(_))
    }

    /// Get as text, if this is a text message
    pub fn as_text(&self) -> Option<&str> {
        match self {
            WsMessage::Text(t) => Some(t.as_str()),
            _ => None,
        }
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            WsMessage::Text(t) => Some(t.as_ref()),
            WsMessage::Binary(b) => Some(b),
            WsMessage::Close(_) => None,
        }
    }
}

impl From<WsText> for WsMessage {
    fn from(text: WsText) -> Self {
        WsMessage::Text(text)
    }
}

impl From<String> for WsMessage {
    fn from(s: String) -> Self {
        WsMessage::Text(WsText::from(s))
    }
}

impl From<&str> for WsMessage {
    fn from(s: &str) -> Self {
        WsMessage::Text(WsText::from(s))
    }
}

impl From<Bytes> for WsMessage {
    fn from(bytes: Bytes) -> Self {
        WsMessage::Binary(bytes)
    }
}

impl From<Vec<u8>> for WsMessage {
    fn from(vec: Vec<u8>) -> Self {
        WsMessage::Binary(Bytes::from(vec))
    }
}

/// WebSocket message stream
#[cfg(not(target_arch = "wasm32"))]
pub struct WsStream(Pin<Box<dyn Stream<Item = Result<WsMessage, StreamError>> + Send>>);

/// WebSocket message stream
#[cfg(target_arch = "wasm32")]
pub struct WsStream(Pin<Box<dyn Stream<Item = Result<WsMessage, StreamError>>>>);

impl WsStream {
    /// Create a new message stream
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<WsMessage, StreamError>> + Send + 'static,
    {
        Self(Box::pin(stream))
    }

    /// Create a new message stream
    #[cfg(target_arch = "wasm32")]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<WsMessage, StreamError>> + 'static,
    {
        Self(Box::pin(stream))
    }

    /// Convert into the inner pinned boxed stream
    #[cfg(not(target_arch = "wasm32"))]
    pub fn into_inner(self) -> Pin<Box<dyn Stream<Item = Result<WsMessage, StreamError>> + Send>> {
        self.0
    }

    /// Convert into the inner pinned boxed stream
    #[cfg(target_arch = "wasm32")]
    pub fn into_inner(self) -> Pin<Box<dyn Stream<Item = Result<WsMessage, StreamError>>>> {
        self.0
    }

    /// Split this stream into two streams that both receive all messages
    ///
    /// Messages are cloned (cheaply via Bytes rc). Spawns a forwarder task.
    /// Both returned streams will receive all messages from the original stream.
    /// The forwarder continues as long as at least one stream is alive.
    /// If the underlying stream errors, both teed streams will end.
    pub fn tee(self) -> (WsStream, WsStream) {
        use futures::channel::mpsc;
        use n0_future::StreamExt as _;

        let (tx1, rx1) = mpsc::unbounded();
        let (tx2, rx2) = mpsc::unbounded();

        n0_future::task::spawn(async move {
            let mut stream = self.0;
            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => {
                        // Clone message (cheap - Bytes is rc'd)
                        let msg2 = msg.clone();

                        // Send to both channels, continue if at least one succeeds
                        let send1 = tx1.unbounded_send(Ok(msg));
                        let send2 = tx2.unbounded_send(Ok(msg2));

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

        (WsStream::new(rx1), WsStream::new(rx2))
    }
}

impl fmt::Debug for WsStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WsStream").finish_non_exhaustive()
    }
}

/// WebSocket message sink
#[cfg(not(target_arch = "wasm32"))]
pub struct WsSink(Pin<Box<dyn n0_future::Sink<WsMessage, Error = StreamError> + Send>>);

/// WebSocket message sink
#[cfg(target_arch = "wasm32")]
pub struct WsSink(Pin<Box<dyn n0_future::Sink<WsMessage, Error = StreamError>>>);

impl WsSink {
    /// Create a new message sink
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<S>(sink: S) -> Self
    where
        S: n0_future::Sink<WsMessage, Error = StreamError> + Send + 'static,
    {
        Self(Box::pin(sink))
    }

    /// Create a new message sink
    #[cfg(target_arch = "wasm32")]
    pub fn new<S>(sink: S) -> Self
    where
        S: n0_future::Sink<WsMessage, Error = StreamError> + 'static,
    {
        Self(Box::pin(sink))
    }

    /// Convert into the inner boxed sink
    #[cfg(not(target_arch = "wasm32"))]
    pub fn into_inner(
        self,
    ) -> Pin<Box<dyn n0_future::Sink<WsMessage, Error = StreamError> + Send>> {
        self.0
    }

    /// Convert into the inner boxed sink
    #[cfg(target_arch = "wasm32")]
    pub fn into_inner(self) -> Pin<Box<dyn n0_future::Sink<WsMessage, Error = StreamError>>> {
        self.0
    }

    /// get a mutable reference to the inner boxed sink
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_mut(
        &mut self,
    ) -> &mut Pin<Box<dyn n0_future::Sink<WsMessage, Error = StreamError> + Send>> {
        use core::borrow::BorrowMut;

        self.0.borrow_mut()
    }

    /// get a mutable reference to the inner boxed sink
    #[cfg(target_arch = "wasm32")]
    pub fn get_mut(
        &mut self,
    ) -> &mut Pin<Box<dyn n0_future::Sink<WsMessage, Error = StreamError> + 'static>> {
        use core::borrow::BorrowMut;

        self.0.borrow_mut()
    }
}

impl fmt::Debug for WsSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WsSink").finish_non_exhaustive()
    }
}

/// WebSocket client trait
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait WebSocketClient: Sync {
    /// Error type for WebSocket operations
    type Error: core::error::Error + Send + Sync + 'static;

    /// Connect to a WebSocket endpoint
    fn connect(
        &self,
        uri: Uri<&str>,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>>;
    /// Connect to a WebSocket endpoint with headers and subprotocols.
    ///
    /// Protocols are sent via `Sec-WebSocket-Protocol` during the upgrade
    /// handshake; on wasm they must be passed at construction (the browser
    /// API has no post-construction header mechanism), so they never travel
    /// through generic headers.
    ///
    /// Default implementation forwards to `connect()`, dropping both:
    /// clients that do not override this cannot negotiate subprotocols or
    /// send authentication headers, and servers may reject the upgrade.
    fn connect_with_options(
        &self,
        uri: Uri<&str>,
        _options: WebSocketConnectOptions<'_>,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>> {
        async move { self.connect(uri).await }
    }
}

/// Connection options carried through the WebSocket upgrade handshake.
///
/// The browser WebSocket API accepts subprotocols only at construction
/// and offers no custom headers; transports backed by it ignore
/// [`Self::headers`] and native implementations should forward them.
#[derive(Debug, Clone, Default)]
pub struct WebSocketConnectOptions<'a> {
    /// Extra HTTP headers for the upgrade request.
    pub headers: Vec<(CowStr<'a>, CowStr<'a>)>,
    /// Subprotocols to offer via `Sec-WebSocket-Protocol`.
    pub protocols: Vec<&'a str>,
}

/// WebSocket connection with bidirectional streams
pub struct WebSocketConnection {
    tx: WsSink,
    rx: WsStream,
}

impl WebSocketConnection {
    /// Create a new WebSocket connection
    pub fn new(tx: WsSink, rx: WsStream) -> Self {
        Self { tx, rx }
    }

    /// Get mutable access to the sender
    pub fn sender_mut(&mut self) -> &mut WsSink {
        &mut self.tx
    }

    /// Get mutable access to the receiver
    pub fn receiver_mut(&mut self) -> &mut WsStream {
        &mut self.rx
    }

    /// Get a reference to the receiver
    pub fn receiver(&self) -> &WsStream {
        &self.rx
    }

    /// Get a reference to the sender
    pub fn sender(&self) -> &WsSink {
        &self.tx
    }

    /// Split into sender and receiver
    pub fn split(self) -> (WsSink, WsStream) {
        (self.tx, self.rx)
    }

    /// Check if connection is open (always true for this abstraction)
    pub fn is_open(&self) -> bool {
        true
    }
}

impl fmt::Debug for WebSocketConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketConnection")
            .finish_non_exhaustive()
    }
}

/// Concrete WebSocket client implementation using tokio-tungstenite-wasm
pub mod tungstenite_client {
    use super::*;
    use futures::{SinkExt, StreamExt};

    /// WebSocket client backed by tokio-tungstenite-wasm
    #[derive(Debug, Clone, Default)]
    pub struct TungsteniteClient;

    impl TungsteniteClient {
        /// Create a new tungstenite WebSocket client
        pub fn new() -> Self {
            Self
        }

        /// Wrap a connected stream in a [`WebSocketConnection`].
        fn wrap_stream(
            &self,
            ws_stream: tokio_tungstenite_wasm::WebSocketStream,
        ) -> Result<WebSocketConnection, WebSocketError> {
            let (sink, stream) = ws_stream.split();

            // Convert tungstenite messages to our WsMessage
            let rx_stream = stream.filter_map(|result| async move {
                match result {
                    Ok(msg) => match convert_message(msg) {
                        Some(ws_msg) => Some(Ok(ws_msg)),
                        None => None, // Skip ping/pong
                    },
                    Err(e) => Some(Err(StreamError::transport(e))),
                }
            });

            let rx = WsStream::new(rx_stream);

            // Convert our WsMessage to tungstenite messages
            let tx_sink =
                sink.with(|msg: WsMessage| async move { Ok::<_, WebSocketError>(msg.into()) });

            let tx_sink_mapped = tx_sink.sink_map_err(|e| StreamError::transport(e));
            let tx = WsSink::new(tx_sink_mapped);

            Ok(WebSocketConnection::new(tx, rx))
        }
    }

    impl WebSocketClient for TungsteniteClient {
        type Error = WebSocketError;

        async fn connect(&self, uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
            let ws_stream = tokio_tungstenite_wasm::connect(uri.as_str())
                .await
                .map_err(classify_tungstenite_error)?;
            self.wrap_stream(ws_stream)
        }

        async fn connect_with_options(
            &self,
            uri: Uri<&str>,
            options: WebSocketConnectOptions<'_>,
        ) -> Result<WebSocketConnection, Self::Error> {
            let headers: Vec<(String, String)> = options
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let ws_stream = tokio_tungstenite_wasm::connect_with_headers(
                uri.as_str(),
                &options.protocols,
                &headers,
            )
            .await
            .map_err(classify_tungstenite_error)?;
            self.wrap_stream(ws_stream)
        }
    }

    /// Convert tokio-tungstenite-wasm Message to our WsMessage
    /// Returns None for Ping/Pong which we auto-handle
    fn convert_message(msg: tokio_tungstenite_wasm::Message) -> Option<WsMessage> {
        use tokio_tungstenite_wasm::Message;

        match msg {
            Message::Text(text) => {
                // Zero-copy: Utf8Bytes -> Bytes -> WsText, all re-wraps of the
                // same UTF-8-validated buffer.
                let bytes: Bytes = text.into();
                Some(WsMessage::Text(unsafe {
                    WsText::from_bytes_unchecked(bytes)
                }))
            }
            Message::Binary(vec) => Some(WsMessage::Binary(Bytes::from_owner(vec))),
            Message::Close(frame) => {
                let close_frame = frame.map(|f| {
                    let code = convert_close_code(f.code);
                    // Zero-copy: Utf8Bytes and WsText are both UTF-8-validated
                    // wrappers over bytes::Bytes; re-wrap without touching data.
                    let bytes: Bytes = f.reason.into();
                    CloseFrame::new(code, unsafe { WsText::from_bytes_unchecked(bytes) })
                });
                Some(WsMessage::Close(close_frame))
            }
        }
    }

    /// Convert tokio-tungstenite-wasm CloseCode to our CloseCode
    fn convert_close_code(code: tokio_tungstenite_wasm::CloseCode) -> CloseCode {
        use tokio_tungstenite_wasm::CloseCode as TungsteniteCode;

        match code {
            TungsteniteCode::Normal => CloseCode::Normal,
            TungsteniteCode::Away => CloseCode::Away,
            TungsteniteCode::Protocol => CloseCode::Protocol,
            TungsteniteCode::Unsupported => CloseCode::Unsupported,
            TungsteniteCode::Invalid => CloseCode::Invalid,
            TungsteniteCode::Policy => CloseCode::Policy,
            TungsteniteCode::Size => CloseCode::Size,
            TungsteniteCode::Extension => CloseCode::Extension,
            TungsteniteCode::Error => CloseCode::Error,
            TungsteniteCode::Tls => CloseCode::Tls,
            // For other variants, extract raw code
            other => {
                let raw: u16 = other.into();
                CloseCode::from(raw)
            }
        }
    }

    impl From<WsMessage> for tokio_tungstenite_wasm::Message {
        fn from(msg: WsMessage) -> Self {
            use tokio_tungstenite_wasm::Message;

            match msg {
                WsMessage::Text(text) => {
                    // Safe: WsText is already UTF-8 validated, so this cannot
                    // fail and no copy is made — the buffer moves as-is.
                    let utf8: tokio_tungstenite_wasm::Utf8Bytes = text
                        .into_bytes()
                        .try_into()
                        .expect("WsText is UTF-8 validated");
                    Message::Text(utf8)
                }
                WsMessage::Binary(bytes) => Message::Binary(bytes),
                WsMessage::Close(frame) => {
                    let close_frame = frame.map(|f| {
                        let code = u16::from(f.code).into();
                        // Zero-copy: Bytes -> Utf8Bytes re-wrap (already
                        // UTF-8 validated by WsText).
                        tokio_tungstenite_wasm::CloseFrame {
                            code,
                            reason: f
                                .reason
                                .into_bytes()
                                .try_into()
                                .expect("WsText is UTF-8 validated"),
                        }
                    });
                    Message::Close(close_frame)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_text_from_string() {
        let text = WsText::from("hello");
        assert_eq!(text.as_str(), "hello");
    }

    #[test]
    fn ws_text_deref() {
        let text = WsText::from(String::from("world"));
        assert_eq!(&*text, "world");
    }

    #[test]
    fn ws_text_try_from_bytes() {
        let bytes = Bytes::from("test");
        let text = WsText::try_from(bytes).unwrap();
        assert_eq!(text.as_str(), "test");
    }

    #[test]
    fn ws_text_invalid_utf8() {
        let bytes = Bytes::from(vec![0xFF, 0xFE]);
        assert!(WsText::try_from(bytes).is_err());
    }

    #[test]
    fn ws_message_text() {
        let msg = WsMessage::from("hello");
        assert!(msg.is_text());
        assert_eq!(msg.as_text(), Some("hello"));
    }

    #[test]
    fn ws_message_binary() {
        let msg = WsMessage::from(vec![1, 2, 3]);
        assert!(msg.is_binary());
        assert_eq!(msg.as_bytes(), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn close_code_conversion() {
        assert_eq!(u16::from(CloseCode::Normal), 1000);
        assert_eq!(CloseCode::from(1000), CloseCode::Normal);
        assert_eq!(CloseCode::from(9999), CloseCode::Other(9999));
    }

    #[test]
    fn websocket_connection_has_tx_and_rx() {
        use futures::sink::SinkExt;
        use futures::stream;

        let rx_stream = stream::iter(vec![Ok(WsMessage::from("test"))]);
        let rx = WsStream::new(rx_stream);

        let drain_sink = futures::sink::drain()
            .sink_map_err(|_: std::convert::Infallible| StreamError::closed());
        let tx = WsSink::new(drain_sink);

        let conn = WebSocketConnection::new(tx, rx);
        assert!(conn.is_open());
    }
}
