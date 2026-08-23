//! # Jacquard
//!
//! A suite of Rust crates intended to make it much easier to get started with atproto development,
//! without sacrificing flexibility or performance.
//!
//! [Jacquard is simpler](https://whtwnd.com/nonbinary.computer/3m33efvsylz2s) because it is
//! designed in a way which makes things simple that almost every other atproto library seems to make difficult.
//!
//!
//! ## Features
//!
//! - Validated, spec-compliant, easy to work with, and performant baseline types.
//! - Designed such that you can just work with generated API bindings easily.
//! - Straightforward OAuth.
//! - Server-side convenience features.
//! - Lexicon Data value type for working with unknown atproto data (dag-cbor or json).
//! - An order of magnitude less boilerplate than some existing crates.
//! - Batteries-included, but easily replaceable batteries.
//!    - Easy to extend with custom lexicons using code generation or handwritten api types.
//!    - Stateless options (or options where you handle the state) for rolling your own.
//!    - All the building blocks of the convenient abstractions are available.
//!    - Use as much or as little from the crates as you need.
//!
//! ## Example
//!
//! Dead simple API client: resume a stored OAuth session or open a browser login, then fetch the
//! latest 5 posts. OAuth loopback is the default path for local scripts and CLIs where browser login
//! is acceptable; app-password credential sessions are mainly for unattended workflows that must
//! re-authenticate non-interactively.
//!
//! ```no_run
//! use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
//! use jacquard::client::{Agent, FileAuthStore};
//! use jacquard::common::session::SessionHint;
//! use jacquard::oauth::client::OAuthClient;
//! use jacquard::xrpc::XrpcClient;
//! use jacquard::oauth::types::AuthorizeOptions;
//! # #[cfg(feature = "loopback")]
//! use jacquard::oauth::loopback::LoopbackConfig;
//! # use miette::IntoDiagnostic;
//!
//! const STORE_PATH: &str = "/tmp/jacquard-oauth-session.json";
//!
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let login_hint = std::env::args().nth(1);
//!     let oauth = OAuthClient::with_default_config(FileAuthStore::new(STORE_PATH));
//!     let hint = SessionHint::from_optional_input(login_hint.as_deref());
//!
//! #   #[cfg(feature = "loopback")]
//!     let Some(session) = oauth
//!        .resume_or_login_with_local_server(
//!            &hint,
//!            AuthorizeOptions::default(),
//!            LoopbackConfig::default(),
//!        )
//!        .await?
//!     else {
//!         miette::bail!(
//!             "no stored OAuth session found in {STORE_PATH}; pass a handle, DID, or PDS URL to log in"
//!         );
//!     };
//! #   #[cfg(not(feature = "loopback"))]
//! #   compile_error!("loopback feature must be enabled to run this example");
//!
//!     let agent: Agent<_> = Agent::from(session);
//!     let timeline = agent
//!         .send(GetTimeline::new().limit(5).build())
//!         .await?
//!         .into_output()?;
//!
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
//! - [`jacquard-repo`](https://docs.rs/jacquard-repo/latest/jacquard_repo/index.html) - Repository primitives (MST, commits, CAR I/O, block storage)
//! - [`jacquard-lexicon`](https://docs.rs/jacquard-lexicon/latest/jacquard_lexicon/index.html) - Lexicon resolution, fetching, parsing and Rust code generation from schemas
//! - [`jacquard-lexgen`](https://docs.rs/jacquard-lexicon/latest/jacquard_lexicon/index.html) - Code generation binaries
//! - [`jacquard-derive`](https://docs.rs/jacquard-derive/latest/jacquard_derive/index.html) - Macros (`#[lexicon]`, `#[open_union]`, `#[derive(IntoStatic)]`, `#[derive(LexiconSchema)]`, `#[derive(XrpcRequest)]`)
//!
//!
//! ### String backing types
//!
//! Most generated Jacquard types are parameterized over a string backing type: `Type<S = DefaultStr>`.
//! The default backing is owned and efficient. It is especially convenient when values need to be
//! stored, moved independently of a response buffer, or passed through frameworks and APIs with
//! `DeserializeOwned` bounds. In most examples you will not write the `S` parameter at all because
//! builders, constructors, and `.into_output()` infer or choose the owned default for you.
//!
//! When you are writing generic helpers or optimizing parsing, you can choose another backing such
//! as `String`, `&str`, or [`CowStr<'_>`] with the [`BosStr`] trait. For API responses, use
//! `.into_output()` as the normal path for owned/default-backed output. Use
//! `.parse::<CowStr<'_>>()` or `.parse::<&str>()` when you specifically want to borrow from the
//! response buffer.
//!
//! [`BosStr`]: crate::BosStr
//!
//! ## Client options
//!
//! - Stateless XRPC: any `HttpClient` (e.g., `reqwest::Client`) implements `XrpcExt`,
//!   which provides `xrpc(base: Uri<String>) -> XrpcCall` for per-request calls with
//!   optional `CallOptions` (auth, proxy, labelers, headers). Useful when you
//!   want to pass auth on each call or build advanced flows.
//!  ```no_run
//!   #  use jacquard::xrpc::XrpcExt;
//!   #  use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
//!   #  use jacquard::types::ident::AtIdentifier;
//!   #  use jacquard::deps::fluent_uri::Uri;
//!   #  use miette::IntoDiagnostic;
//!   #
//!   #[tokio::main]
//!   async fn main() -> miette::Result<()> {
//!       let http = reqwest::Client::new();
//!       let base = Uri::parse("https://public.api.bsky.app").into_diagnostic()?;
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
//! # use jacquard::deps::fluent_uri::Uri;
//! # use miette::IntoDiagnostic;
//! #
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let http = reqwest::Client::new();
//!     let base = Uri::parse("https://public.api.bsky.app").into_diagnostic()?;
//!     let resp = http
//!         .xrpc(base)
//!         .auth(AuthorizationToken::Bearer("ACCESS_JWT".into()))
//!         .accept_labelers(vec!["did:plc:labelerid".into()])
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
//! [`AgentSessionExt`]: crate::client::AgentSessionExt

#![warn(missing_docs)]

pub mod client;

#[cfg(feature = "streaming")]
/// Streaming endpoints
pub mod streaming;
/// Jetstream v2: `.jss` archive decoding and replay orchestration.
///
/// Requires `streaming` (the generated `subscribeRepos`/`subscribeEvents`
/// types are codegen-gated on it); the block-frame reader additionally
/// requires `zstd`, since `.jss` frames are always zstd-compressed
/// upstream.
#[cfg(feature = "streaming")]
pub mod jetstream;

#[cfg(feature = "api_bluesky")]
pub mod richtext;

#[cfg(feature = "api")]
pub mod moderation;

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
    pub use crate::client::Agent;
    pub use crate::client::AgentSession;
    #[cfg(feature = "api")]
    pub use crate::client::AgentSessionExt;
    #[cfg(feature = "reqwest-client")]
    pub use crate::client::BasicClient;
    pub use crate::common::http_client::HttpClient;
    pub use crate::common::xrpc::XrpcClient;
    pub use crate::common::xrpc::XrpcExt;
    pub use crate::identity::JacquardResolver;
    pub use crate::identity::resolver::IdentityResolver;
    pub use crate::oauth::dpop::DpopExt;
    pub use crate::oauth::resolver::OAuthResolver;
}
