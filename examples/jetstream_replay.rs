//! Example: Replay a Jetstream v2 archive with cutover to the live stream
//!
//! Plans the sealed archive from a persisted cursor, backfills matching
//! events, then cuts over to `subscribeEvents` with seq dedupe at the
//! handover. The cursor is persisted to a local file between runs, so
//! restarting resumes from the last delivered event.
//!
//! Usage (against a real public instance; the archive endpoints are
//! metered and require an API key):
//!   cargo run --example jetstream_replay --features streaming,zstd
//!   cargo run --example jetstream_replay --features streaming,zstd -- \
//!       jetstream.us-east.bsky.network <api-key> \
//!       --collections 'app.bsky.feed.*'
//!
//! Against the e2e container, point the first argument at the exported
//! provider host and omit the API key. Persisted cursors are written only
//! after an event is delivered; production handlers should apply the event
//! durably before advancing the cursor.

use clap::Parser;
use jacquard::jetstream::archive::JetstreamClient;
use jacquard::jetstream::plan::{CollectionFilter, EventKind, ReplayFilters};
use jacquard::jetstream::replay::{ReplayMode, ReplayOptions, ReplayStream};
use jacquard_api::network_bsky::jetstream::subscribe_events::SubscribeEventsMessage;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::websocket::tungstenite_client::TungsteniteClient;
use miette::IntoDiagnostic;
use smol_str::SmolStr;

#[derive(Parser, Debug)]
#[command(about = "Replay a Jetstream v2 archive with live cutover")]
struct Args {
    /// Jetstream instance host (e.g. jetstream.us-east.bsky.network).
    host: SmolStr,
    /// Archive API key. Required by public instances; omit for
    /// self-hosted ones.
    api_key: Option<SmolStr>,
    /// Collection filter: an NSID or a namespace wildcard like
    /// app.bsky.feed.*
    #[arg(long)]
    collections: Vec<SmolStr>,
    /// Cursor file for resuming between runs.
    #[arg(long, default_value = "jetstream-cursor.txt")]
    cursor_file: std::path::PathBuf,
    /// Snapshot only: stop at the sealed tip instead of cutting over.
    #[arg(long)]
    snapshot: bool,
}

fn load_cursor(path: &std::path::Path) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn store_cursor(path: &std::path::Path, seq: i64) {
    if let Err(e) = std::fs::write(path, seq.to_string()) {
        eprintln!("could not persist cursor: {e}");
    }
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let base = Uri::parse(format!("https://{}", args.host))
        .map_err(|(e, input)| miette::miette!("invalid host {:?}: {e}", input))?;
    let client = JetstreamClient::new(reqwest::Client::new(), base, args.api_key);

    let filters = ReplayFilters {
        kinds: vec![EventKind::Commit],
        dids: Vec::new(),
        collections: args
            .collections
            .into_iter()
            .map(|c| CollectionFilter::parse(c).unwrap())
            .collect(),
    };

    let after_seq = load_cursor(&args.cursor_file);
    let mode = if args.snapshot {
        ReplayMode::Snapshot {
            after_seq,
            before_seq: None,
        }
    } else {
        ReplayMode::Replay { after_seq }
    };

    let mut stream = ReplayStream::new(
        client,
        TungsteniteClient::new(),
        filters,
        mode,
        ReplayOptions::default(),
    );

    let mut count = 0u64;
    while let Some(item) = stream.next().await.into_diagnostic()? {
        if let Some(seq) = item.last_seq {
            store_cursor(&args.cursor_file, seq);
        }
        count += 1;
        if let SubscribeEventsMessage::Commit(commit) = &item.message {
            println!(
                "{} {} {}",
                item.last_seq.unwrap_or_default(),
                commit.did,
                commit.collection.as_ref()
            );
        }
        if count % 1000 == 0 {
            eprintln!(
                "delivered {count} events (cursor {:?})",
                stream.checkpoint()
            );
        }
    }
    eprintln!("stream ended after {count} events");
    Ok(())
}
