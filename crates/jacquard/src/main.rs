use clap::Parser;
use jacquard::client::{AuthenticatedClient, Session, XrpcClient};
use jacquard_api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard_api::com_atproto::server::create_session::CreateSession;
use jacquard_common::CowStr;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - AT Protocol client demo")]
struct Args {
    /// Username/handle (e.g., alice.bsky.social)
    #[arg(short, long)]
    username: CowStr<'static>,

    /// PDS URL (e.g., https://bsky.social)
    #[arg(long, default_value = "https://bsky.social")]
    pds: CowStr<'static>,

    /// App password
    #[arg(short, long)]
    password: CowStr<'static>,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Create HTTP client
    let http = reqwest::Client::new();
    let mut client = AuthenticatedClient::new(http, args.pds);

    // Create session
    println!("logging in as {}...", args.username);
    let create_session = CreateSession::new()
        .identifier(args.username)
        .password(args.password)
        .build();

    let session_output = client.send(create_session).await?.into_output()?;
    let session = Session::from(session_output);

    println!("logged in as {} ({})", session.handle, session.did);
    client.set_session(session);

    // Fetch timeline
    println!("\nfetching timeline...");
    let timeline_req = GetTimeline::new().limit(5).build();

    let timeline = client.send(timeline_req).await?.into_output()?;

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
