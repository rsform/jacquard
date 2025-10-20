use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::api::app_bsky::labeler::get_services::GetServicesOutput;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::cowstr::ToCowStr;
use jacquard::from_data;
use jacquard::moderation::{Blur, Moderateable, ModerationPrefs, fetch_labeler_defs};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::xrpc::{CallOptions, XrpcClient};
use jacquard_api::app_bsky::feed::{ReplyRefParent, ReplyRefRoot};
use jacquard_api::app_bsky::labeler::get_services::GetServicesOutputViewsItem;

// To save having to fetch prefs, etc., we're borrowing some from our test cases.
const LABELER_SERVICES_JSON: &str =
    include_str!("../crates/jacquard/src/moderation/labeler_services.json");

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Fetch timeline with moderation labels applied"
)]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,

    /// Number of posts to fetch
    #[arg(short, long, default_value = "50")]
    limit: i64,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Extract labeler DIDs from the static JSON (used for testing)
    let services: GetServicesOutput<'static> =
        serde_json::from_str(LABELER_SERVICES_JSON).expect("failed to parse labeler services");

    let mut accepted_labelers = Vec::new();

    for view in services.views {
        if let GetServicesOutputViewsItem::LabelerViewDetailed(detailed) = view {
            accepted_labelers.push(detailed.creator.did.clone());
        }
    }

    println!(
        "Fetching live definitions for {} labelers...",
        accepted_labelers.len()
    );

    // OAuth login
    let store = FileAuthStore::new(&args.store);
    let client_data = jacquard_oauth::session::ClientData {
        keyset: None,
        config: AtprotoClientMetadata::default_localhost(),
    };

    let oauth = OAuthClient::new(store, client_data);
    let session = oauth
        .login_with_local_server(
            args.input.clone(),
            Default::default(),
            LoopbackConfig::default(),
        )
        .await?;

    let agent: Agent<_> = Agent::from(session);

    // Fetch live labeler definitions from the network
    let defs = fetch_labeler_defs(&agent, accepted_labelers.clone()).await?;

    println!("Loaded definitions for {} labelers\n", defs.defs.len());

    // Fetch timeline with labelers enabled via CallOptions
    let mut opts = CallOptions::default();
    opts.atproto_accept_labelers = Some(
        accepted_labelers
            .iter()
            .map(|did| did.to_cowstr())
            .collect(),
    );
    let request = GetTimeline::new().limit(args.limit).build();

    println!("\nFetching timeline with {} posts...\n", args.limit);

    let response = agent.send_with_opts(request, opts).await?;
    let timeline = response.into_output()?;

    // Apply moderation preferences (default: no adult content)
    let prefs = ModerationPrefs::default();

    let mut filtered = 0;
    let mut warned = 0;
    let mut clean = 0;

    for feed_post in timeline.feed.iter() {
        let post = &feed_post.post;

        // Use Moderateable trait to get moderation decisions for all parts
        // (post, author, reply chain)
        let decisions = feed_post.moderate_all(&prefs, &defs, &accepted_labelers);

        // Determine overall status from all decisions
        if decisions.iter().any(|(_, d)| d.filter) {
            filtered += 1;
        } else if decisions
            .iter()
            .any(|(_, d)| d.blur != Blur::None || d.alert)
        {
            warned += 1;
        } else {
            clean += 1;
        }

        let text = from_data::<Post>(&post.record)
            .inspect_err(|e| println!("error: {e}"))
            .ok()
            .map(|p| p.text.to_string())
            .unwrap_or_else(|| "<no text>".to_string());

        if let Some(reply) = &feed_post.reply {
            if let ReplyRefParent::PostView(parent) = &reply.parent {
                if let ReplyRefRoot::PostView(root) = &reply.root {
                    if root.uri != parent.uri {
                        let root_text = from_data::<Post>(&root.record)
                            .ok()
                            .map(|p| p.text.to_string())
                            .unwrap_or_else(|| "<no text>".to_string());
                        println!("@{}:\n{}", root.author.handle, root_text);
                    }
                }
                let parent_text = from_data::<Post>(&parent.record)
                    .ok()
                    .map(|p| p.text.to_string())
                    .unwrap_or_else(|| "<no text>".to_string());
                println!("@{}:\n{}", parent.author.handle, parent_text);
            }
        }
        println!("@{}:\n{}", post.author.handle, text);

        // Show details for any part with moderation causes
        for (tag, decision) in decisions.iter() {
            if !decision.causes.is_empty() {
                println!(
                    "   {}: {:?}",
                    tag,
                    decision
                        .causes
                        .iter()
                        .map(|c| c.label.as_str())
                        .collect::<Vec<_>>()
                );
                if decision.filter {
                    println!("      → Would be hidden");
                } else if decision.blur != Blur::None {
                    println!("      → Would be blurred ({:?})", decision.blur);
                }
                if decision.alert {
                    println!("      → Alert-level warning");
                }
                if decision.no_override {
                    println!("      → User cannot override");
                }
            }
        }
    }

    println!("\n--- Summary ---");
    println!("Total posts: {}", timeline.feed.len());
    println!("Clean: {}", clean);
    println!("Warned: {}", warned);
    println!("Filtered: {}", filtered);

    Ok(())
}
