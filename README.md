# Jacquard

A suite of Rust crates for the AT Protocol.

[![Crates.io](https://img.shields.io/crates/v/jacquard.svg)](https://crates.io/crates/jacquard) [![Documentation](https://docs.rs/jacquard/badge.svg)](https://docs.rs/jacquard) [![License](https://img.shields.io/crates/l/jacquard.svg)](./LICENSE)

## Goals

- Validated, spec-compliant, easy to work with, and performant baseline types (including typed at:// uris)
- Batteries-included, but easily replaceable batteries.
  - Easy to extend with custom lexicons
- lexicon Value type for working with unknown atproto data (dag-cbor or json)
- order of magnitude less boilerplate than some existing crates
  - either the codegen produces code that's easy to work with, or there are good handwritten wrappers
- didDoc type with helper methods for getting handles, multikey, and PDS endpoint
- use as much or as little from the crates as you need


## Example

Dead simple API client. Logs in with an app password and prints the latest 5 posts from your timeline.

```rust
use std::sync::Arc;
use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::client::credential_session::{CredentialSession, SessionKey};
use jacquard::client::{AtpSession, FileAuthStore, MemorySessionStore};
use jacquard::identity::PublicResolver as JacquardResolver;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - AT Protocol client demo")]
struct Args {
    /// Username/handle (e.g., alice.bsky.social) or DID
    #[arg(short, long)]
    username: CowStr<'static>,
    /// App password
    #[arg(short, long)]
    password: CowStr<'static>,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Resolver + storage
    let resolver = Arc::new(JacquardResolver::default());
    let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
    let client = Arc::new(resolver.clone());
    let session = CredentialSession::new(store, client);

    // Login (resolves PDS automatically) and persist as (did, "session")
    session
        .login(args.username.clone(), args.password.clone(), None, None, None)
        .await
        .into_diagnostic()?;

    // Fetch timeline
    let timeline = session
        .clone()
        .send(GetTimeline::new().limit(5).build())
        .await
        .into_diagnostic()?
        .into_output()
        .into_diagnostic()?;

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
```

## Development

This repo uses [Flakes](https://nixos.asia/en/flakes) from the get-go.

```bash
# Dev shell
nix develop

# or run via cargo
nix develop -c cargo run

# build
nix build
```

There's also a [`justfile`](https://just.systems/) for Makefile-esque commands to be run inside of the devShell, and you can generally `cargo ...` or `just ...` whatever just fine if you don't want to use Nix and have the prerequisites installed.
