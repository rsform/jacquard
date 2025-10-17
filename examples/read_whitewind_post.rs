use clap::Parser;
use jacquard::api::com_whtwnd::blog::entry::Entry;
use jacquard::client::{AgentSessionExt, BasicClient};
use jacquard::types::string::AtUri;

#[derive(Parser, Debug)]
#[command(author, version, about = "Read a WhiteWind blog post")]
struct Args {
    /// at:// URI of the blog entry
    /// Example: at://did:plc:xyz/com.whtwnd.blog.entry/3l5abc123
    uri: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Parse the at:// URI
    let uri = AtUri::new(&args.uri)?;

    // Create an unauthenticated agent for public record access
    let agent = BasicClient::unauthenticated();

    // Use Agent's get_record helper with the at:// URI
    let response = agent.get_record::<Entry>(&uri).await?;
    let output = response.into_output()?;

    println!("📚 WhiteWind Blog Entry\n");
    println!("URI: {}", output.uri);
    println!(
        "Title: {}",
        output.value.title.as_deref().unwrap_or("[Untitled]")
    );
    if let Some(subtitle) = &output.value.subtitle {
        println!("Subtitle: {}", subtitle);
    }
    if let Some(created) = &output.value.created_at {
        println!("Created: {}", created);
    }
    println!("\n{}", output.value.content);

    Ok(())
}
