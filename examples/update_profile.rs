use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::actor::profile::Profile;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::types::string::AtUri;
use jacquard::xrpc::XrpcClient;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Update profile display name and description")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// New display name
    #[arg(long)]
    display_name: Option<String>,

    /// New bio/description
    #[arg(long)]
    description: Option<String>,

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

    // Get session info to build the at:// URI for the profile record
    let (did, _) = agent
        .info()
        .await
        .ok_or_else(|| miette::miette!("No session info available"))?;

    // Profile records use "self" as the rkey
    let uri_string = format!("at://{}/app.bsky.actor.profile/self", did);
    let uri = AtUri::new(&uri_string)?;

    // Update profile in-place using the fetch-modify-put pattern
    agent
        .update_record::<Profile>(uri, |profile| {
            if let Some(name) = &args.display_name {
                profile.display_name = Some(CowStr::from(name.clone()));
            }
            if let Some(desc) = &args.description {
                profile.description = Some(CowStr::from(desc.clone()));
            }
        })
        .await?;

    println!("✓ Profile updated successfully");

    Ok(())
}
