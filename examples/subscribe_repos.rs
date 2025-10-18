//! Example: Subscribe to a PDS's subscribeRepos endpoint
//!
//! This demonstrates consuming the repo event stream directly from a PDS,
//! which is what a Relay does to ingest updates from PDSes.
//!
//! Usage:
//!   cargo run --example subscribe_repos -- atproto.systems

use clap::Parser;
use jacquard::api::com_atproto::sync::subscribe_repos::{SubscribeRepos, SubscribeReposMessage};
use jacquard_common::xrpc::{SubscriptionClient, TungsteniteSubscriptionClient};
use miette::IntoDiagnostic;
use n0_future::StreamExt;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about = "Subscribe to a PDS's subscribeRepos endpoint")]
struct Args {
    /// PDS URL (e.g., atproto.systems or https://atproto.systems)
    pds_url: String,

    /// Starting cursor position
    #[arg(short, long)]
    cursor: Option<i64>,
}

fn normalize_url(input: &str) -> Result<Url, url::ParseError> {
    // Strip any existing scheme
    let without_scheme = input
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("wss://")
        .trim_start_matches("ws://");

    // Prepend wss://
    Url::parse(&format!("wss://{}", without_scheme))
}

fn print_message(msg: &SubscribeReposMessage) {
    match msg {
        SubscribeReposMessage::Commit(commit) => {
            println!(
                "Commit | repo={} seq={} time={} rev={} commit={} ops={} prev={}",
                commit.repo,
                commit.seq,
                commit.time,
                commit.rev,
                commit.commit,
                commit.ops.len(),
                commit.since,
            );
        }
        SubscribeReposMessage::Identity(identity) => {
            println!(
                "Identity | did={} seq={} time={} handle={:?}",
                identity.did, identity.seq, identity.time, identity.handle
            );
        }
        SubscribeReposMessage::Account(account) => {
            println!(
                "Account | did={} seq={} time={} active={} status={:?}",
                account.did, account.seq, account.time, account.active, account.status
            );
        }
        SubscribeReposMessage::Sync(sync) => {
            println!(
                "Sync | did={} seq={} time={} rev={} blocks={}b",
                sync.did,
                sync.seq,
                sync.time,
                sync.rev,
                sync.blocks.len()
            );
        }
        SubscribeReposMessage::Info(info) => {
            println!("Info | name={} message={:?}", info.name, info.message);
        }
        SubscribeReposMessage::Unknown(data) => {
            println!("Unknown message: {:?}", data);
        }
    }
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let base_url = normalize_url(&args.pds_url).into_diagnostic()?;
    println!("Connecting to {}", base_url);

    // Create subscription client
    let client = TungsteniteSubscriptionClient::from_base_uri(base_url);

    // Subscribe with optional cursor
    let params = if let Some(cursor) = args.cursor {
        SubscribeRepos::new().cursor(cursor).build()
    } else {
        SubscribeRepos::new().build()
    };
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

    loop {
        tokio::select! {
            Some(result) = messages.next() => {
                match result {
                    Ok(msg) => print_message(&msg),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            _ = &mut rx => {
                println!("\nShutting down...");
                break;
            }
        }
    }

    Ok(())
}
