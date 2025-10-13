use clap::Parser;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::types::string::Datetime;
use jacquard::xrpc::XrpcClient;
use jacquard::CowStr;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create a simple post")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Post text
    #[arg(short, long)]
    text: String,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
    let session = oauth
        .login_with_local_server(args.input, Default::default(), LoopbackConfig::default())
        .await?;

    let agent: Agent<_> = Agent::from(session);

    // Create a simple text post using the Agent convenience method
    let post = Post {
        text: CowStr::from(args.text),
        created_at: Datetime::now(),
        embed: None,
        entities: None,
        facets: None,
        labels: None,
        langs: None,
        reply: None,
        tags: None,
        extra_data: Default::default(),
    };

    let output = agent.create_record(post, None).await?;
    println!("✓ Created post: {}", output.uri);

    Ok(())
}
