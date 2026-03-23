use clap::Parser;
use jacquard::CowStr;
use jacquard::api::com_whtwnd::blog::entry::Entry;
use jacquard::client::{Agent, AgentSessionExt, FileAuthStore};
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::types::string::Datetime;
use jacquard_common::deps::fluent_uri::Uri;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create a WhiteWind blog post")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Blog post title
    #[arg(short, long)]
    title: String,

    /// Blog post content (markdown)
    #[arg(short, long)]
    content: String,

    /// Optional subtitle
    #[arg(short, long)]
    subtitle: Option<String>,

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

    // Create a WhiteWind blog entry
    // The content field accepts markdown
    let entry = Entry {
        title: Some(args.title),
        subtitle: args.subtitle,
        content: args.content,
        created_at: Some(Datetime::now()),
        visibility: Some(String::from("url")), // "url" = public with link, "author" = public on profile
        theme: None,
        ogp: None,
        blobs: None,
        is_draft: None,
        extra_data: Default::default(),
    };

    let output = agent.create_record(entry, None).await?;
    println!("Created WhiteWind blog post: {}", output.uri);
    let url_str = format!(
        "https://whtwnd.nat.vg/{}/{}",
        output.uri.authority(),
        output.uri.rkey().map(|r| r.as_ref()).unwrap_or("")
    );
    let url = Uri::parse(url_str.as_str()).into_diagnostic()?;
    println!("View at: {}", url);

    Ok(())
}
