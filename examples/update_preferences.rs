use clap::Parser;
use jacquard::api::app_bsky::actor::{AdultContentPref, PreferencesItem};
use jacquard::client::vec_update::VecUpdate;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::{CowStr, IntoStatic};

#[derive(Parser, Debug)]
#[command(author, version, about = "Update Bluesky preferences")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Enable adult content
    #[arg(long)]
    enable_adult_content: bool,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

/// Helper struct for the VecUpdate pattern on preferences
pub struct PreferencesUpdate;

impl VecUpdate for PreferencesUpdate {
    type GetRequest<'de> = jacquard::api::app_bsky::actor::get_preferences::GetPreferences;
    type PutRequest<'de> = jacquard::api::app_bsky::actor::put_preferences::PutPreferences<'de>;
    type Item = PreferencesItem<'static>;

    fn build_get<'s>() -> Self::GetRequest<'s> {
        jacquard::api::app_bsky::actor::get_preferences::GetPreferences::new().build()
    }

    fn build_put<'s>(items: Vec<Self::Item>) -> Self::PutRequest<'s> {
        jacquard::api::app_bsky::actor::put_preferences::PutPreferences {
            preferences: items,
            extra_data: Default::default(),
        }
    }

    fn extract_vec(
        output: jacquard::api::app_bsky::actor::get_preferences::GetPreferencesOutput<'_>,
    ) -> Vec<Self::Item> {
        output
            .preferences
            .into_iter()
            .map(|p| p.into_static())
            .collect()
    }

    fn matches(a: &Self::Item, b: &Self::Item) -> bool {
        // Match by enum variant discriminant
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
    let session = oauth
        .login_with_local_server(args.input, Default::default(), LoopbackConfig::default())
        .await?;

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
