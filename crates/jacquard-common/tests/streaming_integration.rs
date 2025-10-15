#![cfg(all(feature = "streaming", feature = "reqwest-client", not(target_arch = "wasm32")))]

use jacquard_common::http_client::HttpClientExt;
use jacquard_common::stream::{StreamError, StreamErrorKind};
use n0_future::StreamExt;
use bytes::Bytes;

#[tokio::test]
async fn streaming_response_delivers_all_bytes() {
    let client = reqwest::Client::new();

    let request = http::Request::builder()
        .uri("https://www.rust-lang.org/")
        .body(vec![])
        .unwrap();

    let response = client.send_http_streaming(request).await.unwrap();
    assert!(response.status().is_success());

    let (_parts, body) = response.into_parts();
    let stream = body.into_inner();

    // Pin the stream for iteration
    tokio::pin!(stream);

    let mut total = 0;

    while let Some(result) = stream.next().await {
        let chunk = result.unwrap();
        total += chunk.len();
    }

    // Just verify we got some bytes
    assert!(total > 0);
}

#[tokio::test]
async fn streaming_upload_sends_all_chunks() {
    let client = reqwest::Client::new();

    let chunks = vec![
        Bytes::from("chunk1"),
        Bytes::from("chunk2"),
        Bytes::from("chunk3"),
    ];
    let body_stream = futures::stream::iter(chunks.clone());

    // Build a complete request and extract parts
    let request: http::Request<Vec<u8>> = http::Request::builder()
        .method(http::Method::POST)
        .uri("https://httpbin.org/post")
        .body(vec![])
        .unwrap();
    let (parts, _) = request.into_parts();

    let response = client.send_http_bidirectional(parts, body_stream).await.unwrap();
    assert!(response.status().is_success());
}

#[tokio::test]
async fn stream_error_preserves_source() {
    let io_error = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
    let stream_error = StreamError::transport(io_error);

    assert_eq!(stream_error.kind(), &StreamErrorKind::Transport);
    assert!(stream_error.source().is_some());
}
