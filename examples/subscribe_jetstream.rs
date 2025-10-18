//! Example: Subscribe to Jetstream firehose
//!
//! Jetstream is a JSON-based alternative to the standard DAG-CBOR firehose.
//! It streams all public network updates in a simplified format.
//!
//! Usage:
//!   cargo run --example subscribe_jetstream
//!   cargo run --example subscribe_jetstream -- jetstream2.us-west.bsky.network

use clap::Parser;
use jacquard_common::jetstream::{CommitOperation, JetstreamMessage, JetstreamParams};
use jacquard_common::xrpc::{SubscriptionClient, TungsteniteSubscriptionClient};
use miette::IntoDiagnostic;
use n0_future::StreamExt;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about = "Subscribe to Jetstream firehose")]
struct Args {
    /// Jetstream URL (e.g., jetstream1.us-east.fire.hose.cam)
    #[arg(default_value = "jetstream1.us-east.fire.hose.cam")]
    jetstream_url: String,
}

fn normalize_url(input: &str) -> Result<Url, url::ParseError> {
    // Strip any existing scheme
    let without_scheme = input
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("wss://")
        .trim_start_matches("ws://");

    // Prepend wss:// and parse
    Url::parse(&format!("wss://{}", without_scheme))
}

fn print_message(msg: &JetstreamMessage) {
    match msg {
        JetstreamMessage::Commit {
            did,
            time_us,
            commit,
        } => {
            let op = match commit.operation {
                CommitOperation::Create => "create",
                CommitOperation::Update => "update",
                CommitOperation::Delete => "delete",
            };
            println!(
                "Commit | did={} time_us={} op={} collection={} rkey={} cid={:?}",
                did, time_us, op, commit.collection, commit.rkey, commit.cid
            );
        }
        JetstreamMessage::Identity {
            did,
            time_us,
            identity,
        } => {
            println!(
                "Identity | did={} time_us={} handle={:?} seq={} time={}",
                did, time_us, identity.handle, identity.seq, identity.time
            );
        }
        JetstreamMessage::Account {
            did,
            time_us,
            account,
        } => {
            println!(
                "Account | did={} time_us={} active={} seq={} time={} status={:?}",
                did, time_us, account.active, account.seq, account.time, account.status
            );
        }
    }
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let base_url = normalize_url(&args.jetstream_url).into_diagnostic()?;
    println!("Connecting to {}", base_url);

    // Create subscription client
    let client = TungsteniteSubscriptionClient::from_base_uri(base_url);

    // Subscribe with no filters (firehose mode)
    // Enable compression if zstd feature is available
    #[cfg(feature = "zstd")]
    let params = { JetstreamParams::new().compress(true).build() };

    #[cfg(not(feature = "zstd"))]
    let params = { JetstreamParams::new().build() };

    let stream = client.subscribe(&params).await.into_diagnostic()?;

    println!("Connected! Streaming messages (Ctrl-C to stop)...\n");

    // Set up Ctrl-C handler
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = tx.send(());
    });

    // Convert to typed message stream
    let (_sink, mut messages) = stream.into_stream();

    let mut count = 0u64;

    loop {
        tokio::select! {
            Some(result) = messages.next() => {
                match result {
                    Ok(msg) => {
                        count += 1;
                        print_message(&msg);
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            _ = &mut rx => {
                println!("\nReceived {} messages", count);
                println!("Shutting down...");
                break;
            }
        }
    }

    Ok(())
}
