# jacquard-common

Core AT Protocol types and HTTP client abstraction for Jacquard.

## Features

### `streaming` (optional)

Adds support for streaming HTTP request and response bodies:

- `ByteStream` / `ByteSink`: Platform-agnostic stream abstractions
- `HttpClientExt`: Streaming methods for HTTP client
- `StreamingResponse`: XRPC streaming response wrapper
- Error types: `StreamError` with `StreamErrorKind`

Works on both native and WASM targets via `n0-future`.

**Example:**
```rust
use jacquard_common::http_client::{HttpClient, HttpClientExt};
use futures::StreamExt;

let client = reqwest::Client::new();
let response = client.send_http_streaming(request).await?;

let stream = response.into_parts().1.into_inner();
futures::pin_mut!(stream);
while let Some(chunk) = stream.next().await {
    // Process streaming chunks
}
```

### `websocket` (optional)

Adds WebSocket client abstraction:

- `WebSocketClient` trait
- `WebSocketConnection` with bidirectional streams
- Uses same `ByteStream`/`ByteSink` as HTTP streaming

Requires the `streaming` feature.

### `reqwest-client` (optional)

Implements `HttpClient` and `HttpClientExt` for `reqwest::Client`.
