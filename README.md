# Jacquard

A suite of Rust crates for the AT Protocol.

## Example

Dead simple api client. Logs in, prints the latest 5 posts from your timeline.

```rust
use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::api::com_atproto::server::create_session::CreateSession;
use jacquard::client::{AuthenticatedClient, Session, XrpcClient};
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - AT Protocol client demo")]
struct Args {
    /// Username/handle (e.g., alice.mosphere.at)
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
```

## Goals

- Validated, spec-compliant, easy to work with, and performant baseline types (including typed at:// uris)
- Batteries-included, but easily replaceable batteries.
  - Easy to extend with custom lexicons
- lexicon Value type for working with unknown atproto data (dag-cbor or json)
- order of magnitude less boilerplate than some existing crates
  - either the codegen produces code that's easy to work with, or there are good handwritten wrappers
- didDoc type with helper methods for getting handles, multikey, and PDS endpoint
- use as much or as little from the crates as you need

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
