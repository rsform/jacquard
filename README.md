[![Crates.io](https://img.shields.io/crates/v/jacquard.svg)](https://crates.io/crates/jacquard) [![Documentation](https://docs.rs/jacquard/badge.svg)](https://docs.rs/jacquard) [![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/S6S8170HQ8)

# Jacquard

A suite of Rust crates intended to make it much easier to get started with atproto development, without sacrificing flexibility or performance.

[Jacquard is simpler](https://alpha.weaver.sh/nonbinary.computer/jacquard/jacquard_magic) because it is designed in a way which makes things simple that almost every other atproto library seems to make difficult.

Jacquard generated types are generic over their string backing, but ordinary client code can usually ignore that detail. Use the generated request builders, pass normal strings, and call `.send(...).into_output()?` to get owned output that is easy to store, move independently of the response buffer, and pass through frameworks or APIs that require `DeserializeOwned`. If you need tighter control later, Jacquard still supports borrowing and zero-copy parsing with backing types such as `&str` and `CowStr<'_>`.


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


## Example

Dead simple API client. Resumes a stored OAuth session or opens a browser login, then prints the latest 5 posts from your timeline. This is the default path for local scripts and CLIs where browser login is acceptable; app-password credential sessions are mainly for unattended workflows that must re-authenticate non-interactively.

```rust
// Note: this requires the `loopback` feature enabled (it is currently by default).
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::oauth::types::AuthorizeOptions;
use jacquard::xrpc::XrpcClient;
use miette::IntoDiagnostic;

const STORE_PATH: &str = "/tmp/jacquard-oauth-session.json";

#[tokio::main]
async fn main() -> miette::Result<()> {
    let login_hint = std::env::args().nth(1);
    let oauth = OAuthClient::with_default_config(FileAuthStore::new(STORE_PATH));
    let hint = SessionHint::from_optional_input(login_hint.as_deref());

    let Some(session) = oauth
        .resume_or_login_with_local_server(
            &hint,
            AuthorizeOptions::default(),
            LoopbackConfig::default(),
        )
        .await?
    else {
        miette::bail!(
            "no stored OAuth session found in {STORE_PATH}; pass a handle, DID, or PDS URL to log in"
        );
    };

    let agent: Agent<_> = Agent::from(session);
    let timeline = agent
        .send(GetTimeline::new().limit(5).build())
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

If you have `just` installed, you can run the [examples](https://tangled.org/nonbinary.computer/jacquard/tree/main/examples) using `just example {example-name} {ARGS}` or `just examples` to see what's available.

> [!WARNING]
> The latest version swaps from the `url` crate to the lighter and quicker `fluent-uri`. It also moves the re-exported crate paths around and renames the `Uri<'_>` value type enum to `UriValue<'_>` to avoid confusion. This is likely to have broken some things. Migrating is pretty straightforward but consider yourself forewarned. This crate is *not* 1.0 for a reason.

### Changelog

[CHANGELOG.md](./CHANGELOG.md)

#### 0.11 Release Highlights:

- `jacquard-lexgen` and `jacquard-identity` no longer depend on the generated API crate. This is mostly for my own benefit.

**Code generation pipeline overhaul** (`jacquard-lexicon`, `jacquard-lexgen`)
- Jacquard's codegen output already was nice to *use*. now it's going to be nice to read.
- New code generation tracks the types used, makes an import block for the file, and then organizes the file with stuff you care about at the top and internal stuff, like the builders, at the bottom.
- Import resolution pass now conditionally generates short paths when types are unambiguous within a module, falling back to fully-qualified paths when collisions exist

#### 0.10 Release Highlights:

**URL type migration**
- Migrated from `url` crate to `fluent_uri` for validated URL/URI types
- All `Url` types are now `Uri` from `fluent_uri`
- Affects any code that constructs, passes, or pattern-matches on endpoint URLs

**Re-exported crate paths**
- Re-exported crates (including non-proc-macro dependencies of the generated API crate) are now centralized into a distinct module
- Import paths for re-exported types have changed

**`no_std` groundwork** 
- Initial work toward allowing jacquard to function on platforms without access to the standard library.
- `std` usage is now feature-gated. the library currently *does not compile* without `std` due to some remaining dependencies.

### Projects using Jacquard

- [Tranquil PDS](https://tangled.org/tranquil.farm/tranquil-pds)
- [skywatch-phash-rs](https://tangled.org/skywatch.blue/skywatch-phash-rs)
- [Weaver](https://weaver.sh/) - [tangled repository](https://tangled.org/nonbinary.computer/weaver)
- [wisp.place CLI tool](https://docs.wisp.place/cli/) - formerly
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

### Testimonials

- ["the most straightforward interface to atproto I've encountered so far."](https://bsky.app/profile/offline.mountainherder.xyz/post/3m3xwewzs3k2v) - @offline.mountainherder.xyz
- "It has saved me a lot of time already! Well worth a few beers and or microcontrollers" - [@baileytownsend.dev](https://bsky.app/profile/baileytownsend.dev)
- ["This is what your library allowed me to do in an hour!!! Thank you!!!"](https://bsky.app/profile/desertthunder.dev/post/3mhhbcula6224) - @desertthunder.dev


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
