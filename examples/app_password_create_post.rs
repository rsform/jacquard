use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::client::{Agent, AgentSessionExt, MemoryCredentialSession};
use jacquard::types::string::Datetime;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create a simple post")]
struct Args {
    /// Handle (e.g., alice.bsky.social) or DID
    input: CowStr<'static>,

    /// App Password
    password: CowStr<'static>,

    /// Post text
    #[arg(short, long)]
    text: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let (session, auth) =
        MemoryCredentialSession::authenticated(args.input, args.password, None).await?;
    println!("Signed in as {}", auth.handle);

    let agent: Agent<_> = Agent::from(session);

    // Create a simple text post using the Agent convenience method
    let post = Post::builder().text(args.text).build();
    let output = agent.create_record(post, None).await?;
    println!("✓ Created post: {}", output.uri);

    Ok(())
}
