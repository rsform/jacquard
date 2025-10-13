use jacquard::api::app_bsky::feed::get_feed::GetFeed;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::types::string::AtUri;
use jacquard::types::value::from_data;
use jacquard::xrpc::XrpcExt;
use miette::IntoDiagnostic;

#[tokio::main]
async fn main() -> miette::Result<()> {
    // Stateless XRPC - no auth required for public feeds
    let http = reqwest::Client::new();
    let base = url::Url::parse("https://public.api.bsky.app").into_diagnostic()?;

    // Feed of posts about the AT Protocol
    let feed_uri =
        AtUri::new_static("at://did:plc:oio4hkxaop4ao4wz2pp3f4cr/app.bsky.feed.generator/atproto")
            .unwrap();

    let request = GetFeed::new().feed(feed_uri).limit(10).build();

    let response = http.xrpc(base).send(&request).await?;
    let output = response.into_output()?;

    println!("Latest posts from the AT Protocol feed:\n");
    for (i, item) in output.feed.iter().enumerate() {
        // Deserialize the post record from the Data type
        let post: Post = from_data(&item.post.record).into_diagnostic()?;
        println!("{}.(@{})\n{} ", i + 1, item.post.author.handle, post.text);
    }

    Ok(())
}
