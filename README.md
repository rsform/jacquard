[![Crates.io](https://img.shields.io/crates/v/jacquard.svg)](https://crates.io/crates/jacquard) [![Documentation](https://docs.rs/jacquard/badge.svg)](https://docs.rs/jacquard)

# Jacquard

A suite of Rust crates intended to make it much easier to get started with atproto development, without sacrificing flexibility or performance.

[Jacquard is simpler](https://whtwnd.com/nonbinary.computer/3m33efvsylz2s) because it is designed in a way which makes things simple that almost every other atproto library seems to make difficult.

It is also designed around zero-copy/borrowed deserialization: types like [`Post<'_>`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-api/src/app_bsky/feed/post.rs) can borrow data (via the [`CowStr<'_>`](https://docs.rs/jacquard/latest/jacquard/cowstr/enum.CowStr.html) type and a host of other types built on top of it) directly from the response buffer instead of allocating owned copies. Owned versions are themselves mostly inlined or reference-counted pointers and are therefore still quite efficient. The `IntoStatic` trait (which is derivable) makes it easy to get an owned version and avoid worrying about lifetimes.

## Features

- Validated, spec-compliant, easy to work with, and performant baseline types
- Designed such that you can just work with generated API bindings easily
- Straightforward OAuth
- Server-side convenience features
- Lexicon Data value type for working with unknown atproto data (dag-cbor or json)
- An order of magnitude less boilerplate than some existing crates
- Batteries-included, but easily replaceable batteries.
   - Easy to extend with custom lexicons using code generation or handwritten api types
   - Stateless options (or options where you handle the state) for rolling your own
   - All the building blocks of the convenient abstractions are available
   - Use as much or as little from the crates as you need

## 0.9.0 Release Highlights:

**`#[derive(LexiconSchema)]` + `#[lexicon_union]` macros**
- Automatic schema generation for custom lexicons from Rust structs
- Supports all lexicon constraints via attributes (max_length, max_graphemes, min/max, etc.)
- Generates `LexiconDoc` at compile time for runtime validation

**Runtime lexicon data validation**
- Validation of structural and/or value contraints of data against a lexicon
- caching for value validations
- LexiconSchema trait generated implementations for runtime validation
- detailed validation error results

**Lexicon resolver**
- Fetch lexicons at runtime for addition to schema registry

**Query and path DSLs for `Data` and `RawData` value types**
- Pattern-based querying of nested `Data` structures
- `data.query(pattern)` with expressive syntax:
  - `field.nested` - exact path navigation
  - `[..]` - wildcard over collections (array elements or object values)
  - `field..nested` - scoped recursion (find nested within field, expect one)
  - `...field` - global recursion (find all occurrences anywhere)
- `get_at_path()` for simple path-based field access on `Data` and `RawData`
- Path syntax: `embed.images[0].alt` for navigating nested structures
- `type_discriminator()` helper methods for AT Protocol union discrimination
- Collection helper methods: `get()`, `contains_key()`, `len()`, `is_empty()`, `iter()`, `keys()`, `values()`
- Index trait implemented: `obj["key"]` and `arr[0]`

**Caching in identity/lexicon resolver**
- Basic LRU in-memory cache implementation using `mini-moka`
- Reduces number of network requests for certain operations
- Works on both native and WebAssembly
- **NOTE** wasm target for `mini-moka` requires a git dependency, use the git version of the crate when compiling for wasm

**XRPC client improvements**
- `set_options()` and `set_endpoint()` methods on `XrpcClient` trait
- Default no-op implementations for stateless clients
- Enables runtime reconfiguration of stateful clients
- Better support for custom endpoint and option overrides
- Fixed bug where setting a custom 'Content-Type' header wouldn't be respected

**Major generated API compilation time improvements**
- Generated code output now includes a typestate builder implementation, similar to the `bon` crate
- Moves the substantial `syn` tax of generating the builders to code generation time, not compile time.

**New `jacquard-lexgen` crate**
- Moves binaries out of jacquard-lexicon to reduce size further
- Flake app for `lex-fetch`

## Example

Dead simple API client. Logs in with OAuth and prints the latest 5 posts from your timeline.

```rust
// Note: this requires the `loopback` feature enabled (it is currently by default)
use clap::Parser;
use jacquard::CowStr;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::types::xrpc::XrpcClient;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - OAuth (DPoP) loopback demo")]
struct Args {
    /// Handle (e.g., alice.bsky.social), DID, or PDS URL
    input: CowStr<'static>,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Build an OAuth client with file-backed auth store and default localhost config
    let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
    // Authenticate with a PDS, using a loopback server to handle the callback flow
    let session = oauth
        .login_with_local_server(
            args.input.clone(),
            Default::default(),
            LoopbackConfig::default(),
        )
        .await?;
    // Wrap in Agent and fetch the timeline
    let agent: Agent<_> = Agent::from(session);
    let timeline = agent
        .send(&GetTimeline::new().limit(5).build())
        .await?
        .into_output()?;
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

If you have `just` installed, you can run the [examples](https://tangled.org/@nonbinary.computer/jacquard/tree/main/examples) using `just example {example-name} {ARGS}` or `just examples` to see what's available.

> [!WARNING]
> A lot of the streaming code is still pretty experimental. The examples work, though.\
The modules are also less well-documented, and don't have code examples. There are also a lot of utility functions for conveniently working with the streams and transforming them which are lacking. Use [`n0-future`](https://docs.rs/n0-future/latest/n0_future/index.html) to work with them, that is what Jacquard uses internally as much as possible.\
>I would also note the same for the repository crate until I've had more third parties test it.

### Changelog

[CHANGELOG.md](./CHANGELOG.md)

<!--### Testimonials

- ["the most straightforward interface to atproto I've encountered so far."](https://bsky.app/profile/offline.mountainherder.xyz/post/3m3xwewzs3k2v) - @offline.mountainherder.xyz

- "It has saved me a lot of time already! Well worth a few beers and or microcontrollers" - [@baileytownsend.dev](https://bsky.app/profile/baileytownsend.dev)-->

### Projects using Jacquard

- [skywatch-phash-rs](https://tangled.org/skywatch.blue/skywatch-phash-rs)
- [Weaver](https://alpha.weaver.sh/) - [tangled repository](https://tangled.org/nonbinary.computer/weaver)
- [wisp.place CLI tool](https://docs.wisp.place/cli/)
- [PDS MOOver](https://pdsmoover.com/) - [tangled repository](https://tangled.org/baileytownsend.dev/pds-moover)

## Component crates

Jacquard is broken up into several crates for modularity. The correct one to use is generally `jacquard` itself, as it re-exports most of the others.

| | | |
| --- | --- | --- |
| `jacquard` | Main crate | [![Crates.io](https://img.shields.io/crates/v/jacquard.svg)](https://crates.io/crates/jacquard) [![Documentation](https://docs.rs/jacquard/badge.svg)](https://docs.rs/jacquard) |
|`jacquard-common` | Foundation crate | [![Crates.io](https://img.shields.io/crates/v/jacquard-common.svg)](https://crates.io/crates/jacquard-common) [![Documentation](https://docs.rs/jacquard-common/badge.svg)](https://docs.rs/jacquard-common)|
| `jacquard-axum` | Axum extractor and other helpers | [![Crates.io](https://img.shields.io/crates/v/jacquard-axum.svg)](https://crates.io/crates/jacquard-axum) [![Documentation](https://docs.rs/jacquard-axum/badge.svg)](https://docs.rs/jacquard-axum) |
| `jacquard-api` | Autogenerated API bindings | [![Crates.io](https://img.shields.io/crates/v/jacquard-api.svg)](https://crates.io/crates/jacquard-api) [![Documentation](https://docs.rs/jacquard-api/badge.svg)](https://docs.rs/jacquard-api) |
| `jacquard-oauth` | atproto OAuth implementation | [![Crates.io](https://img.shields.io/crates/v/jacquard-oauth.svg)](https://crates.io/crates/jacquard-oauth) [![Documentation](https://docs.rs/jacquard-oauth/badge.svg)](https://docs.rs/jacquard-oauth) |
| `jacquard-identity` | Identity resolution | [![Crates.io](https://img.shields.io/crates/v/jacquard-identity.svg)](https://crates.io/crates/jacquard-identity) [![Documentation](https://docs.rs/jacquard-identity/badge.svg)](https://docs.rs/jacquard-identity) |
| `jacquard-repo` | Repository primitives (MST, commits, CAR I/O) | [![Crates.io](https://img.shields.io/crates/v/jacquard-repo.svg)](https://crates.io/crates/jacquard-repo) [![Documentation](https://docs.rs/jacquard-repo/badge.svg)](https://docs.rs/jacquard-repo) |
| `jacquard-lexicon` | Lexicon parsing and code generation | [![Crates.io](https://img.shields.io/crates/v/jacquard-lexicon.svg)](https://crates.io/crates/jacquard-lexicon) [![Documentation](https://docs.rs/jacquard-lexicon/badge.svg)](https://docs.rs/jacquard-lexicon) |
| `jacquard-lexgen` | Code generation binaries | [![Crates.io](https://img.shields.io/crates/v/jacquard-lexgen.svg)](https://crates.io/crates/jacquard-lexgen) [![Documentation](https://docs.rs/jacquard-lexgen/badge.svg)](https://docs.rs/jacquard-lexgen) |
| `jacquard-derive` | Macros for lexicon types | [![Crates.io](https://img.shields.io/crates/v/jacquard-derive.svg)](https://crates.io/crates/jacquard-derive) [![Documentation](https://docs.rs/jacquard-derive/badge.svg)](https://docs.rs/jacquard-derive) |

## Development

This repo uses [Flakes](https://nixos.asia/en/flakes)

```bash
# Dev shell
nix develop

# or run via cargo
nix develop -c cargo run

# build
nix build
```

There's also a [`justfile`](https://just.systems/) for Makefile-esque commands to be run inside of the devShell, and you can generally `cargo ...` or `just ...` whatever just fine if you don't want to use Nix and have the prerequisites installed.

[![License](https://img.shields.io/crates/l/jacquard.svg)](./LICENSE)
