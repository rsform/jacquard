//! # Jacquard
//!
//! A suite of Rust crates intended to make it much easier to get started with atproto development,
//! without sacrificing flexibility or performance.
//!
//! [Jacquard is simpler](https://whtwnd.com/nonbinary.computer/3m33efvsylz2s) because it is
//! designed in a way which makes things simple that almost every other atproto library seems to make difficult.
//!
//! It is also designed around zero-copy/borrowed deserialization: types like [`Post<'_>`](https://docs.rs/jacquard-api/latest/jacquard_api/app_bsky/feed/post/struct.Post.html) can borrow data (via the [`CowStr<'_>`](https://docs.rs/jacquard/latest/jacquard/cowstr/enum.CowStr.html) type and a host of other types built on top of it) directly from the response buffer instead of allocating owned copies. Owned versions are themselves mostly inlined or reference-counted pointers and are therefore still quite efficient. The `IntoStatic` trait (which is derivable) makes it easy to get an owned version and avoid worrying about lifetimes.
//!
//!
//! ## Goals and Features
//!
//! - Validated, spec-compliant, easy to work with, and performant baseline types
//! - Batteries-included, but easily replaceable batteries.
//!    - Easy to extend with custom lexicons using code generation or handwritten api types
//!    - Straightforward OAuth
//!    - Stateless options (or options where you handle the state) for rolling your own
//!    - All the building blocks of the convenient abstractions are available
//!    - Server-side convenience features
//! - Lexicon Data value type for working with unknown atproto data (dag-cbor or json)
//! - An order of magnitude less boilerplate than some existing crates
//! - Use as much or as little from the crates as you need
//!
//!
//!
//!
//! ## Example
//!
//! Dead simple API client: login with OAuth, then fetch the latest 5 posts.
//!
//! ```no_run
//! # use clap::Parser;
//! # use jacquard::CowStr;
//! use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
//! use jacquard::client::{Agent, FileAuthStore};
//! use jacquard::oauth::client::OAuthClient;
//! use jacquard::xrpc::XrpcClient;
//! # #[cfg(feature = "loopback")]
//! use jacquard::oauth::loopback::LoopbackConfig;
//! # use miette::IntoDiagnostic;
//!
//! # #[derive(Parser, Debug)]
//! # #[command(author, version, about = "Jacquard - OAuth (DPoP) loopback demo")]
//! # struct Args {
//! #     /// Handle (e.g., alice.bsky.social), DID, or PDS URL
//! #     input: CowStr<'static>,
//! #
//! #     /// Path to auth store file (will be created if missing)
//! #     #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
//! #     store: String,
//! # }
//! #
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let args = Args::parse();
//!
//!     // Build an OAuth client with file-backed auth store and default localhost config
//!     let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
//!     // Authenticate with a PDS, using a loopback server to handle the callback flow
//! #   #[cfg(feature = "loopback")]
//!     let session = oauth
//!        .login_with_local_server(
//!            args.input.clone(),
//!            Default::default(),
//!            LoopbackConfig::default(),
//!        )
//!        .await?;
//! #   #[cfg(not(feature = "loopback"))]
//! #   compile_error!("loopback feature must be enabled to run this example");
//!     // Wrap in Agent and fetch the timeline
//!     let agent: Agent<_> = Agent::from(session);
//!     let timeline = agent
//!         .send(GetTimeline::new().limit(5).build())
//!         .await?
//!         .into_output()?;
//!     for (i, post) in timeline.feed.iter().enumerate() {
//!         println!("\n{}. by {}", i + 1, post.post.author.handle);
//!         println!(
//!             "   {}",
//!             serde_json::to_string_pretty(&post.post.record).into_diagnostic()?
//!         );
//!     }
//!     Ok(())
//!}
//! ```
//!
//!
//! ## Component crates
//!
//! Jacquard is split into several crates for modularity. The main `jacquard` crate
//! re-exports most of the others, so you typically only need to depend on it directly.
//!
//! - [`jacquard-common`](https://docs.rs/jacquard-common/latest/jacquard_common/index.html) - AT Protocol types (DIDs, handles, at-URIs, NSIDs, TIDs, CIDs, etc.)
//! - [`jacquard-api`](https://docs.rs/jacquard-api/latest/jacquard_api/index.html) - Generated API bindings from 646+ lexicon schemas
//! - [`jacquard-axum`](https://docs.rs/jacquard-axum/latest/jacquard_axum/index.html) - Server-side XRPC handler extractors for Axum framework (not re-exported, depends on jacquard)
//! - [`jacquard-oauth`](https://docs.rs/jacquard-oauth/latest/jacquard_oauth/index.html) - OAuth/DPoP flow implementation with session management
//! - [`jacquard-identity`](https://docs.rs/jacquard-identity/latest/jacquard_identity/index.html) - Identity resolution (handle → DID, DID → Doc, OAuth metadata)
//! - [`jacquard-lexicon`](https://docs.rs/jacquard-lexicon/latest/jacquard_lexicon/index.html) - Lexicon resolution, fetching, parsing and Rust code generation from schemas
//! - [`jacquard-derive`](https://docs.rs/jacquard-derive/latest/jacquard_derive/index.html) - Macros (`#[lexicon]`, `#[open_union]`, `#[derive(IntoStatic)]`)
//!
//!
//! ### A note on lifetimes
//!
//! You'll notice a bunch of lifetimes all over Jacquard types, examples, and so on. If you're newer
//! to Rust or have simply avoided them, they're part of how Rust knows how long to keep something
//! around before cleaning it up. They're not unique to Rust (C and C++ have the same concept
//! internally) but Rust is perhaps the one language that makes them explicit, because they're part
//! of how it validates that things are memory-safe, and being able to give information to the compiler
//! about how long it can expect something to stick around lets the compiler reason out much more
//! sophisticated things. [The Rust book](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) has a section on them if you want a refresher.
//!
//! > On Jacquard types like [`CowStr`], a `'static` lifetime parameter is used to refer to the owned
//! version of a type, in the same way `String` is the owned version of `&str`.
//!
//! This is somewhat in tension with the 'make things simpler' goal of the crate, but it is honestly
//! pretty straightforward once you know the deal, and Jacquard provides a number of escape hatches
//! and easy ways to work.
//!
//! Because explicit lifetimes are somewhat unique to Rust and are not something you may be used to
//! thinking about, they can seem a bit scary to work with. Normally the compiler is pretty good at
//! them, but Jacquard is [built around borrowed deserialization](https://docs.rs/jacquard-common/latest/jacquard_common/#working-with-lifetimes-and-zero-copy-deserialization) and types. This is for reasons of
//! speed and efficiency, because borrowing from your source buffer saves copying the data around.
//!
//! However, it does mean that any Jacquard type that can borrow (not all of them do) is annotated
//! with a lifetime, to confirm that all the borrowed bits are ["covariant"](https://doc.rust-lang.org/nomicon/subtyping.html), i.e. that they all live
//! at least the same amount of time, and that lifetime matches or exceeds the lifetime of the data
//! structure. This also imposes certain restrictions on deserialization. Namely the [`DeserializeOwned`](https://serde.rs/lifetimes.html)
//! bound does not apply to almost any types in Jacquard. There is a [`deserialize_owned`] function
//! which you can use in a serde `deserialize_with` attribute to help, but the general pattern is
//! to do borrowed deserialization and then call [`.into_static()`] if you need ownership.
//!
//! ### Easy mode
//!
//! Easy mode for jacquard is to mostly just use `'static` for your lifetime params and derive/use
//! [`.into_static()`] as needed. When writing, first see if you can get away with `Thing<'_>`
//! and let the compiler infer. second-easiest after that is `Thing<'static>`, third-easiest is giving
//! everything one lifetime, e.g. `fn foo<'a>(&'a self, thing: Thing<'a>) -> /* thing with lifetime 'a */`.
//!
//! When parsing the output of atproto API calls, you can call `.into_output()` on the `Response<R>`
//! struct to get an owned version with a `'static` lifetime. When deserializing, do not use
//! `from_writer()` type deserialization functions, or features like Axum's `Json` extractor, as they
//! have DeserializeOwned bounds and cannot borrow from their buffer. Either use Jacquard's features
//! to get an owned version or follow the same [patterns](https://whtwnd.com/nonbinary.computer/3m33efvsylz2s) it uses in your own code.
//!
//! ## Client options
//!
//! - Stateless XRPC: any `HttpClient` (e.g., `reqwest::Client`) implements `XrpcExt`,
//!   which provides `xrpc(base: Url) -> XrpcCall` for per-request calls with
//!   optional `CallOptions` (auth, proxy, labelers, headers). Useful when you
//!   want to pass auth on each call or build advanced flows.
//!  ```no_run
//!   #  use jacquard::xrpc::XrpcExt;
//!   #  use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
//!   #  use jacquard::types::ident::AtIdentifier;
//!   #  use miette::IntoDiagnostic;
//!   #
//!   #[tokio::main]
//!   async fn main() -> miette::Result<()> {
//!       let http = reqwest::Client::new();
//!       let base = url::Url::parse("https://public.api.bsky.app").into_diagnostic()?;
//!       let resp = http
//!           .xrpc(base)
//!           .send(
//!               &GetAuthorFeed::new()
//!                   .actor(AtIdentifier::new_static("pattern.atproto.systems").unwrap())
//!                   .limit(5)
//!                   .build(),
//!           )
//!           .await?;
//!       let out = resp.into_output()?;
//!       println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
//!       Ok(())
//!   }
//!   ```
//! - Stateful client (app-password): `CredentialSession<S, T>` where `S: SessionStore<(Did, CowStr), AtpSession>` and
//!   `T: IdentityResolver + HttpClient`. It auto-attaches bearer authorization, refreshes on expiry, and updates the
//!   base endpoint to the user's PDS on login/restore.
//! - Stateful client (OAuth): `OAuthClient<S, T>` and `OAuthSession<S, T>` where `S: ClientAuthStore` and
//!   `T: OAuthResolver + HttpClient`. The client is used to authenticate, returning a session which handles authentication and token refresh internally.
//! - `Agent<A: AgentSession>` Session abstracts over the above two options and provides some useful convenience features via the [`AgentSessionExt`] trait.
//!
//! Per-request overrides (stateless)
//! ```no_run
//! # use jacquard::AuthorizationToken;
//! # use jacquard::xrpc::XrpcExt;
//! # use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
//! # use jacquard::types::ident::AtIdentifier;
//! # use jacquard::CowStr;
//! # use miette::IntoDiagnostic;
//! #
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let http = reqwest::Client::new();
//!     let base = url::Url::parse("https://public.api.bsky.app").into_diagnostic()?;
//!     let resp = http
//!         .xrpc(base)
//!         .auth(AuthorizationToken::Bearer(CowStr::from("ACCESS_JWT")))
//!         .accept_labelers(vec![CowStr::from("did:plc:labelerid")])
//!         .header(http::header::USER_AGENT, http::HeaderValue::from_static("jacquard-example"))
//!         .send(
//!             &GetAuthorFeed::new()
//!                 .actor(AtIdentifier::new_static("pattern.atproto.systems").unwrap())
//!                 .limit(5)
//!                 .build(),
//!         )
//!         .await?;
//!     let out = resp.into_output()?;
//!     println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
//!     Ok(())
//! }
//! ```
//!
//! [`deserialize_owned`]: crate::deserialize_owned
//! [`AgentSessionExt`]: crate::client::AgentSessionExt
//! [`.into_static()`]: IntoStatic

#![warn(missing_docs)]

pub mod client;

#[cfg(feature = "streaming")]
/// Experimental streaming endpoints
pub mod streaming;

#[cfg(feature = "api_bluesky")]
/// Rich text utilities for Bluesky posts
pub mod richtext;

pub use common::*;
#[cfg(feature = "api")]
pub use jacquard_api as api;
pub use jacquard_common as common;

#[cfg(feature = "derive")]
pub use jacquard_derive::*;

pub use jacquard_identity as identity;

pub use jacquard_oauth as oauth;

/// Prelude with the extension traits you're likely to want and some other stuff
pub mod prelude {
    pub use crate::client::AgentSession;
    #[cfg(feature = "api")]
    pub use crate::client::AgentSessionExt;
    pub use crate::client::BasicClient;
    pub use crate::common::http_client::HttpClient;
    pub use crate::common::xrpc::XrpcClient;
    pub use crate::common::xrpc::XrpcExt;
    pub use crate::identity::PublicResolver;
    pub use crate::identity::resolver::IdentityResolver;
    pub use crate::oauth::dpop::DpopExt;
    pub use crate::oauth::resolver::OAuthResolver;
}
