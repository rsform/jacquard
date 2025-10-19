use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::client::{Agent, AgentSessionExt, FileAuthStore};
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::richtext::RichText;
use jacquard::types::string::Datetime;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create a post with automatic facet detection")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Post text (can include @mentions, #hashtags, URLs, and [markdown](links))
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

    // Parse richtext with automatic facet detection
    // This detects @mentions, #hashtags, URLs, and [markdown](links)
    let richtext = RichText::parse(&args.text).build_async(&agent).await?;

    println!("Detected {} facets:", richtext.facets.as_ref().map(|f| f.len()).unwrap_or(0));
    if let Some(facets) = &richtext.facets {
        for facet in facets {
            let text_slice = &richtext.text[facet.index.byte_start as usize..facet.index.byte_end as usize];
            println!("  - \"{}\" ({:?})", text_slice, facet.features);
        }
    }

    // Create post with parsed facets
    let post = Post {
        text: richtext.text,
        facets: richtext.facets,
        created_at: Datetime::now(),
        embed: None,
        entities: None,
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
