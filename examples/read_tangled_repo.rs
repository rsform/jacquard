use clap::Parser;
use jacquard::client::{AgentSessionExt, BasicClient};
use jacquard::types::string::AtUri;
use jacquard_api::sh_tangled::repo::RepoRecord;

#[derive(Parser, Debug)]
#[command(author, version, about = "Read a Tangled git repository record")]
struct Args {
    /// at:// URI of the repo record
    /// Example: at://did:plc:xyz/sh.tangled.repo/3lzabc123
    /// The default is the jacquard repository
    #[arg(default_value = "at://did:plc:yfvwmnlztr4dwkb7hwz55r2g/sh.tangled.repo/3lzrya6fcwv22")]
    uri: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Parse the at:// URI
    let uri = AtUri::new(&args.uri)?;

    // Create an unauthenticated agent for public record access
    let agent = BasicClient::unauthenticated();

    // Use Agent's fetch_record helper with the at:// URI & marker struct
    let output = agent.fetch_record(RepoRecord, uri).await?;

    println!("Tangled Repository\n");
    println!("URI: {}", output.uri);
    println!("Name: {}", output.value.name);

    if let Some(desc) = &output.value.description {
        println!("Description: {}", desc);
    }

    println!("Knot: {}", output.value.knot);
    println!("Created: {}", output.value.created_at);

    if let Some(source) = &output.value.source {
        println!("Source: {}", source.as_str());
    }

    if let Some(spindle) = &output.value.spindle {
        println!("CI Spindle: {}", spindle);
    }

    if let Some(labels) = &output.value.labels {
        if !labels.is_empty() {
            println!(
                "Labels available: {}",
                labels
                    .iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    Ok(())
}
