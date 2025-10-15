//! Example: Download large file using streaming
#![cfg(all(feature = "streaming", feature = "reqwest-client"))]

use jacquard_common::http_client::HttpClientExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let request = http::Request::builder()
        .uri("https://httpbin.org/bytes/1024")
        .body(vec![])
        .unwrap();

    let response = client.send_http_streaming(request).await?;
    println!("Status: {}", response.status());
    println!("Headers: {:?}", response.headers());

    let (_parts, _body) = response.into_parts();
    println!("Received streaming response body (ByteStream)");

    // Note: To iterate over chunks, use futures_lite::StreamExt on the pinned inner stream:
    // let mut stream = Box::pin(body.into_inner());
    // while let Some(chunk) = stream.as_mut().try_next().await? { ... }

    Ok(())
}
