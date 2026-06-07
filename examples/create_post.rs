use clap::Parser;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::client::{Agent, AgentSessionExt, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::oauth::types::AuthorizeOptions;
use jacquard::richtext::RichText;
use jacquard::types::string::Datetime;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Create a post with automatic facet detection"
)]
struct Args {
    /// Optional handle, DID, or PDS URL used to resume or start OAuth.
    input: Option<String>,

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

    // Parse richtext with automatic facet detection
    // This detects @mentions, #hashtags, URLs, and [markdown](links)
    let richtext = RichText::parse(&args.text).build_async(&agent).await?;

    println!(
        "Detected {} facets:",
        richtext.facets.as_ref().map(|f| f.len()).unwrap_or(0)
    );
    if let Some(facets) = &richtext.facets {
        for facet in facets {
            let text_slice =
                &richtext.text[facet.index.byte_start as usize..facet.index.byte_end as usize];
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
