#![cfg(all(feature = "streaming", feature = "reqwest-client"))]

use jacquard_common::http_client::HttpClientExt;

#[tokio::test]
async fn reqwest_client_can_stream_response() {
    let client = reqwest::Client::new();

    let request = http::Request::builder()
        .uri("https://www.rust-lang.org/")
        .body(vec![])
        .unwrap();

    let response = client.send_http_streaming(request).await.unwrap();
    // Just verify we got a response - the fact that it didn't error means the streaming works
    assert!(
        response.status().is_success() || response.status().is_redirection(),
        "Status: {}",
        response.status()
    );
}
