use clap::Parser;
use jacquard::client::{AgentSessionExt, BasicClient};
use jacquard_api::sh_tangled::repo::Repo;

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
    let uri = Repo::uri(args.uri)?;

    // Create an unauthenticated agent for public record access
    let agent = BasicClient::unauthenticated();

    // Use Agent's fetch_record helper with typed record URI
    let output: Repo<'_> = agent.fetch_record(&uri).await?.into();

    println!("Tangled Repository\n");
    println!("URI: {}", uri);
    println!("Name: {}", output.name);

    if let Some(desc) = &output.description {
        println!("Description: {}", desc);
    }

    println!("Knot: {}", output.knot);
    println!("Created: {}", output.created_at);

    if let Some(source) = &output.source {
        println!("Source: {}", source.as_str());
    }

    if let Some(spindle) = &output.spindle {
        println!("CI Spindle: {}", spindle);
    }

    if let Some(labels) = &output.labels {
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
