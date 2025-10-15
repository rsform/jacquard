//! WebSocket client abstraction

use crate::stream::{ByteStream, ByteSink};
use std::future::Future;
use url::Url;

/// WebSocket client trait
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait WebSocketClient {
    /// Error type for WebSocket operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Connect to a WebSocket endpoint
    fn connect(
        &self,
        url: Url,
    ) -> impl Future<Output = Result<WebSocketConnection, Self::Error>>;
}

/// WebSocket connection with bidirectional streams
pub struct WebSocketConnection {
    tx: ByteSink,
    rx: ByteStream,
}

impl WebSocketConnection {
    /// Create a new WebSocket connection
    pub fn new(tx: ByteSink, rx: ByteStream) -> Self {
        Self { tx, rx }
    }

    /// Get mutable access to the sender
    pub fn sender_mut(&mut self) -> &mut ByteSink {
        &mut self.tx
    }

    /// Get mutable access to the receiver
    pub fn receiver_mut(&mut self) -> &mut ByteStream {
        &mut self.rx
    }

    /// Split into sender and receiver
    pub fn split(self) -> (ByteSink, ByteStream) {
        (self.tx, self.rx)
    }

    /// Check if connection is open (always true for this abstraction)
    pub fn is_open(&self) -> bool {
        true
    }
}

impl std::fmt::Debug for WebSocketConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketConnection")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::StreamError;

    #[test]
    fn websocket_connection_has_tx_and_rx() {
        use futures::stream;
        use futures::sink::SinkExt;
        use bytes::Bytes;

        let rx_stream = stream::iter(vec![Ok(Bytes::from("test"))]);
        let rx = ByteStream::new(rx_stream);

        // Create a sink that converts Infallible to StreamError
        let drain_sink = futures::sink::drain().sink_map_err(|_: std::convert::Infallible| {
            StreamError::closed()
        });
        let tx = ByteSink::new(drain_sink);

        let conn = WebSocketConnection::new(tx, rx);
        assert!(conn.is_open());
    }
}
