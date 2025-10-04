use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::api::com_atproto::server::create_session::CreateSession;
use jacquard::client::{AuthenticatedClient, Session, XrpcClient};
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
    let mut client = AuthenticatedClient::new(reqwest::Client::new(), args.pds);

    // Create session
    let session = Session::from(
        client
            .send(
                CreateSession::new()
                    .identifier(args.username)
                    .password(args.password)
                    .build(),
            )
            .await?
            .into_output()?,
    );

    println!("logged in as {} ({})", session.handle, session.did);
    client.set_session(session);

    // Fetch timeline
    println!("\nfetching timeline...");
    let timeline = client
        .send(GetTimeline::new().limit(5).build())
        .await?
        .into_output()?;

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
