use clap::Parser;
use jacquard::api::app_bsky::actor::{AdultContentPref, PreferencesItem};
use jacquard::client::AgentSessionExt;
use jacquard::client::vec_update::PreferencesUpdate;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::oauth::types::AuthorizeOptions;
use jacquard_oauth::scopes::Scopes;

#[derive(Parser, Debug)]
#[command(author, version, about = "Update Bluesky preferences")]
struct Args {
    /// Optional handle or DID used to resume a session.
    input: Option<String>,

    /// Enable adult content
    #[arg(long)]
    enable_adult_content: bool,

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
            AuthorizeOptions::default().with_scopes(
                Scopes::builder()
                    .include_aud(
                        "app.bsky.authFullApp",
                        "did:web:public.api.bsky.app#bsky_appview",
                    )?
                    .build()?,
            ),
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

    // Create the adult content preference
    let adult_pref = AdultContentPref {
        enabled: args.enable_adult_content,
        extra_data: Default::default(),
    };

    // Update preferences using update_vec_item
    // This will replace existing AdultContentPref or add it if not present
    agent
        .update_vec_item::<PreferencesUpdate>(PreferencesItem::AdultContentPref(Box::new(
            adult_pref,
        )))
        .await?;

    println!(
        "✓ Updated adult content preference: {}",
        if args.enable_adult_content {
            "enabled"
        } else {
            "disabled"
        }
    );

    Ok(())
}
