use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::client::credential_session::{CredentialSession, SessionKey};
use jacquard::client::{AtpSession, MemorySessionStore};
use jacquard::types::xrpc::XrpcClient;
use jacquard_identity::slingshot_resolver_default;
use miette::IntoDiagnostic;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - AT Protocol client demo")]
struct Args {
    /// Username/handle (e.g., alice.bsky.social) or DID
    #[arg(short, long)]
    username: CowStr<'static>,

    /// App password
    #[arg(short, long)]
    password: CowStr<'static>,
}
#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Resolver + in-memory store
    let resolver = Arc::new(slingshot_resolver_default());
    let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
    let client = Arc::new(resolver.clone());
    let session = CredentialSession::new(store, client);

    let _ = session
        .login(
            args.username.clone(),
            args.password.clone(),
            None,
            None,
            None,
        )
        .await
        .into_diagnostic()?;

    // Fetch timeline
    let timeline = session
        .send(&GetTimeline::new().limit(5).build())
        .await
        .into_diagnostic()?
        .into_output()
        .into_diagnostic()?;

    println!("\ntimeline ({} posts):", timeline.feed.len());
    for (i, post) in timeline.feed.iter().enumerate() {
        println!("\n{}. by {}", i + 1, post.post.author.handle);
        println!(
            "   {}",
            serde_json::to_string_pretty(&post.post.record).into_diagnostic()?
        );
    }

    Ok(())
}
