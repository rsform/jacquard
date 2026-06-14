[![Crates.io](https://img.shields.io/crates/v/jacquard.svg)](https://crates.io/crates/jacquard) [![Documentation](https://docs.rs/jacquard/badge.svg)](https://docs.rs/jacquard) [![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/S6S8170HQ8)

# Jacquard

A suite of Rust crates intended to make it much easier to get started with atproto development, without sacrificing flexibility or performance.

[Jacquard is simpler](https://alpha.weaver.sh/nonbinary.computer/jacquard/jacquard_magic) because it is designed in a way which makes things simple that almost every other atproto library seems to make difficult.


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

> [!WARNING]
> Jacquard 0.12 includes **many** breaking changes from 0.11. The most notable and far-reaching is the borrow-or-share rewrite, but it is *far* from the only API to have changed. Please read the release highlights and the changelog carefully, as well as the documentation. Please report any such issues on Tangled, particularly any regressions in functionality.

## Example

Dead simple API client. Resumes a stored OAuth session or opens a browser login, then prints the latest 5 posts from your timeline.

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

## Changelog

[CHANGELOG.md](./CHANGELOG.md)

### 0.12 Release Highlights:

#### Borrow-or-share
Jacquard 0.12 swaps from lifetime-based `CowStr<'_>`-backed string types to the "borrow-or-share" pattern. Jacquard generated types are now generic over their string backing type, as opposed to having a lifetime, where that backing type is one that implements the requisite traits for the pattern. The default backing type, aliased `DefaultStr`, is `SmolStr`, but any of `CowStr<'_>`, `String`, `&str`, and `Cow<'_, str>` can be used currently. Defaulting to `SmolStr` maintains the niceties it added to `CowStr<'_>`, such as non-allocating construction from static string slices regardless of length, and inlining small strings in all cases while vastly simplifying the common cases where you don't want to deal with lifetimes.

**Type updates**
- Jacquard types backed by owned string types can now meet `DeserializeOwned` trait bounds. 
- New `.borrow()` method on `Did`, `Handle`, `Nsid`, `Rkey`, `RecordKey` returns `Type<&str>` for cheap borrowing (analogous to `Uri::borrow()` from `fluent_uri`)
- New `.convert::<B>()` method for cross-backing-type conversion

**Response parsing** (`jacquard-common`)
- `Response::parse::<S>()`: caller chooses backing type via turbofish
- `Response::into_output()`: returns `SmolStr`-backed owned types via `DeserializeOwned`

**Generated API types** (`jacquard-api`, `jacquard-lexicon`)
- All generated structs/enums: `Foo<S: BosStr = DefaultStr>` with `#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]`
- `#[serde(borrow)]` removed from all generated code
- String field defaults use `FromStaticStr::from_static()` for zero-alloc construction when using `SmolStr` or `CowStr<'_>`
- Error enums: `SmolStr` message fields, no lifetime parameters
- **Generated builders now have two entry points:**
  - `Type::new()` picks `DefaultStr` as the backing type. This avoids awkward turbofishes or explicit annotations in many scenarios where the builder couldn't work out what type it needed to be from the immediate surroundings.
  - `Type::builder()` allows the caller to choose, either explicitly via turbofish, or implicitly via inference if possible, the backing type (the previous behaviour).

**Note:** `RawData<'a>` currently remains lifetime-based, as do a few other mostly internal types.

#### Sessions and OAuth scopes

The internals of sessions have been reworked fairly substantially, with an eye toward making it easier to resume a session. There are also a number of public API changes here, and a series of helpers for both OAuth and app password sessions to make it easier to either resume a session from available information (for file-based sessions, this can include picking up the first available session, if none is specified) or kick to login. Examples demonstrate the use of these helpers.

OAuth scopes have also been substantially refactored to borrow from an internal buffer as much as possible. Jacquard OAuth scopes also now finally support permission sets. A builder has been added, to allow easy construction of scopes from types, including XrpcRequest and Collection types. Examples have again been updated to use correct, minimal scopes and the occasional permission set. Additions to the lexgen tool to provide lexicon and permission set authoring and publishing, lexicon json validation, and similar are pending in a future update.


#### `jacquard-axum`

Reworked XRPC extractor to work with borrow-or-share types. Backing type for the query or body of the input can differ from the handler-visible backing type, to allow for non-overlapping extractor impls for the different backing types so that the potentially borrowed types like `CowStr<'_>` can still be used.

**Service auth**
  - Improved service auth extractor to properly handle 'did:web:for.some.reason.still.blueskyweb.xyz#bsky_appview'-type service ids (thanks @pds.dad)
  - Added default replay protection for `jti` using a `ReplayStore` trait, default-implemented using a `mini-moka` in-memory cache.

**OAuth web helpers**
  - Added OAuth client counterparts to the service auth extractors.
  - API-oriented extractor provides a useful error on auth failure, if auth is required.
  - Browser-oriented extractor redirects unauthenticated users to a configured URL, while passing state to allow returning to the original URL after login.
  - Configurable routes and handlers for common oauth paths


#### Bootstrap types

A number of manual implementations of critical atproto types have been added to `jacquard-common`. This mostly reduces semi-circular internal dependencies that made certain types of updates a pain in the ass (and also reduces dependence on `jacquard-api` in general so you can disable it more easily if you need to), but *does* affect the types used by, among other things, the `AgentSessionExt` extension trait methods and `CredentialSession` internals. They are no longer the same as the `jacquard-api` types.

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

### Which session type should I use?

| Use case | Recommended path | Notes |
|---|---|---|
| Personal script or CLI/TUI where browser login works | Localhost (native/public) OAuth client with loopback helpers | Best default for “I want to log in locally and not think about auth.” |
| User-facing local CLI/desktop app | Localhost (native/public) OAuth client with loopback helpers | Ensure use of appropriate scopes, NOT 'transition::generic'. |
| Client-side browser app / SPA | Public OAuth client without localhost helpers | Host client metadata, use browser redirects, and persist auth state in a browser-storage-backed auth/session store. |
| App server or web app with persistent backend | Confidential OAuth client | Use hosted metadata, stable redirect URIs, server-side secret/session storage, and appropriate scope choices. |
| Long-running unattended job that must re-authenticate non-interactively | App password / credential session, unless it can operate as a confidential OAuth client | App passwords are appropriate when browser login is not acceptable and the process must recover non-interactively. |
| Public unauthenticated reads | Stateless `.xrpc(...)` or `BasicClient::unauthenticated()` | None |
| Custom protocol/session behavior | Lower-level traits and stores | Implement `XrpcClient`, `AgentSession`, `ClientAuthStore`, `SessionStore`, or related traits yourself. |


### Projects using Jacquard

- [Tranquil PDS](https://tangled.org/tranquil.farm/tranquil-pds)
- [skywatch-phash-rs](https://tangled.org/skywatch.blue/skywatch-phash-rs)
- [Weaver](https://weaver.sh/) - [tangled repository](https://tangled.org/nonbinary.computer/weaver)
- [wisp.place CLI tool](https://docs.wisp.place/cli/) - formerly
- [PDS MOOver](https://pdsmoover.com/) - [tangled repository](https://tangled.org/baileytownsend.dev/pds-moover)
- Downloaded 117k times apparently!?

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
