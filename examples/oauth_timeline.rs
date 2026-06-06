use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
#[cfg(feature = "loopback")]
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::xrpc::XrpcClient;
#[cfg(not(feature = "loopback"))]
use jacquard_oauth::types::AuthorizeOptions;
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
    let client_data = jacquard_oauth::session::ClientData {
        keyset: None,
        // Default sets normal localhost redirect URIs and "atproto transition:generic" scopes.
        // The localhost helper will ensure you have at least "atproto" and will fix urls
        config: AtprotoClientMetadata::default_localhost(),
    };

    // Build an OAuth client (this is reusable, and can create multiple sessions)
    let oauth = OAuthClient::new(store, client_data);

    #[cfg(feature = "loopback")]
    // Authenticate with a PDS, using a loopback server to handle the callback flow
    let session = oauth
        .login_with_local_server(
            args.input.clone(),
            Default::default(),
            LoopbackConfig::default(),
        )
        .await?;

    #[cfg(not(feature = "loopback"))]
    let session = {
        use std::io::{BufRead, Write, stdin, stdout};

        let auth_url = oauth
            .start_auth(args.input, AuthorizeOptions::default())
            .await?;

        println!("To authenticate with your PDS, visit:\n{}\n", auth_url);
        print!("\nPaste the callback url here:");
        stdout().lock().flush().into_diagnostic()?;
        let mut url = String::new();
        stdin().lock().read_line(&mut url).into_diagnostic()?;

        let uri = url.trim().parse::<http::Uri>().into_diagnostic()?;
        let params =
            serde_html_form::from_str(uri.query().ok_or(miette::miette!("invalid callback url"))?)
                .into_diagnostic()?;
        oauth.callback(params).await?
    };

    // Wrap in Agent and fetch the timeline
    let agent: Agent<_> = Agent::from(session);
    let output = agent.send(GetTimeline::new().limit(5).build()).await?;
    let timeline = output.into_output()?;
    for (i, post) in timeline.feed.iter().enumerate() {
        println!("\n{}. by {}", i + 1, post.post.author.handle);
        println!(
            "   {}",
            serde_json::to_string_pretty(&post.post.record).into_diagnostic()?
        );
    }

    Ok(())
}
