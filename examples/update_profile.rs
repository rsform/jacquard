use clap::Parser;
use jacquard::api::app_bsky::actor::profile::Profile;
use jacquard::client::{Agent, AgentSessionExt, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::oauth::types::AuthorizeOptions;
use jacquard::types::string::AtUri;
use smol_str::SmolStr;

#[derive(Parser, Debug)]
#[command(author, version, about = "Update profile display name and description")]
struct Args {
    /// Optional handle, DID, or PDS URL used to resume or start OAuth.
    input: Option<String>,

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

    // Get session info to build the at:// URI for the profile record.
    let (did, _) = agent
        .info()
        .await
        .ok_or_else(|| miette::miette!("No session info available"))?;

    // Profile records use "self" as the rkey.
    let uri_string = format!("at://{}/app.bsky.actor.profile/self", did);
    let uri = AtUri::new(uri_string.as_str())?;

    // Update profile in-place using the fetch-modify-put pattern.
    agent
        .update_record::<Profile, _>(&uri, |profile| {
            if let Some(name) = &args.display_name {
                profile.display_name = Some(SmolStr::new(name));
            }
            if let Some(desc) = &args.description {
                profile.description = Some(SmolStr::new(desc));
            }
        })
        .await?;

    println!("✓ Profile updated successfully");

    Ok(())
}
