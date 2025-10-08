use clap::Parser;
use jacquard::CowStr;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::types::xrpc::XrpcClient;
use jacquard_api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard_oauth::atproto::AtprotoClientMetadata;
use jacquard_oauth::client::OAuthClient;
#[cfg(feature = "loopback")]
use jacquard_oauth::loopback::LoopbackConfig;
use jacquard_oauth::scopes::Scope;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - OAuth (DPoP) loopback demo")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // File-backed auth store shared by OAuthClient and session registry
    let store = FileAuthStore::new(&args.store);

    // Minimal localhost client metadata (redirect_uris get set by loopback helper)
    let client_data = jacquard_oauth::session::ClientData {
        keyset: None,
        // scopes: include atproto; redirect_uris will be populated by the loopback helper
        config: AtprotoClientMetadata::new_localhost(None, Some(vec![Scope::Atproto])),
    };

    // Build an OAuth client and run loopback flow
    let oauth = OAuthClient::new(store, client_data);

    #[cfg(feature = "loopback")]
    let session = oauth
        .login_with_local_server(
            args.input.clone(),
            Default::default(),
            LoopbackConfig::default(),
        )
        .await
        .into_diagnostic()?;

    #[cfg(not(feature = "loopback"))]
    compile_error!("loopback feature must be enabled to run this example");

    // Wrap in Agent and call a simple resource endpoint
    let agent: Agent<_> = Agent::from(session);
    let timeline = agent
        .send(&GetTimeline::new().limit(5).build())
        .await
        .into_diagnostic()?
        .into_output()?;
    for (i, post) in timeline.feed.iter().enumerate() {
        println!("\n{}. by {}", i + 1, post.post.author.handle);
        println!(
            "   {}",
            serde_json::to_string_pretty(&post.post.record).into_diagnostic()?
        );
    }

    Ok(())
}
