//! Example: Upload data using streaming request body

use bytes::Bytes;
use futures::stream;
use jacquard_common::http_client::HttpClientExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // Create a stream of data chunks
    let chunks = vec![
        Bytes::from("Hello, "),
        Bytes::from("streaming "),
        Bytes::from("world!"),
    ];
    let body_stream = stream::iter(chunks);

    // Build request and split into parts
    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri("https://httpbin.org/post")
        .body(())
        .unwrap();

    let (parts, _) = request.into_parts();

    let response = client.send_http_bidirectional(parts, body_stream).await?;
    println!("Status: {}", response.status());

    Ok(())
}
