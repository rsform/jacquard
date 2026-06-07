use clap::Parser;
use jacquard::api::app_bsky::embed::images::{Image, Images};
use jacquard::api::app_bsky::feed::post::{Post, PostEmbed};
use jacquard::client::{Agent, AgentSessionExt, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::oauth::types::AuthorizeOptions;
use jacquard::types::blob::MimeType;
use jacquard::types::string::Datetime;
use miette::IntoDiagnostic;
use smol_str::SmolStr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create a post with an image")]
struct Args {
    /// Optional handle, DID, or PDS URL used to resume a session.
    input: Option<String>,

    /// Post text
    #[arg(short, long)]
    text: String,

    /// Path to image file
    #[arg(short, long)]
    image: PathBuf,

    /// Alt text for image
    #[arg(long)]
    alt: Option<String>,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
    let hint = SessionHint::from_optional_input(args.input.as_deref());
    let Some(session) = oauth
        .resume_or_login_with_local_server(
            &hint,
            AuthorizeOptions::default(),
            LoopbackConfig::default(),
        )
        .await?
    else {
        miette::bail!(
            "no stored OAuth session found in {}; pass a handle, DID, or PDS URL to log in",
            args.store
        );
    };

    let agent: Agent<_> = Agent::from(session);

    // Read image file
    let image_data = std::fs::read(&args.image).into_diagnostic()?;

    // Infer mime type from extension
    let mime_str = match args.image.extension().and_then(|s| s.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/jpeg", // default
    };
    let mime_type = MimeType::new(mime_str);

    println!("Uploading image...");
    let blob = agent.upload_blob(image_data, mime_type).await?;

    // Create post with image embed
    let post = Post {
        text: SmolStr::from(args.text),
        created_at: Datetime::now(),
        embed: Some(PostEmbed::Images(Box::new(Images {
            images: vec![Image {
                alt: SmolStr::from(args.alt.unwrap_or_default()),
                image: blob.into(),
                aspect_ratio: None,
                extra_data: Default::default(),
            }],
            extra_data: Default::default(),
        }))),
        entities: None,
        facets: None,
        labels: None,
        langs: None,
        reply: None,
        tags: None,
        extra_data: Default::default(),
    };

    let output = agent.create_record(post, None).await?;
    println!("Created post with image: {}", output.uri);

    Ok(())
}
