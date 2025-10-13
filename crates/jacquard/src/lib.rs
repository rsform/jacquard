//! # Jacquard
//!
//! A suite of Rust crates for the AT Protocol.
//!
//!
//! ## Goals and Features
//!
//! - Validated, spec-compliant, easy to work with, and performant baseline types
//! - Batteries-included, but easily replaceable batteries.
//!   - Easy to extend with custom lexicons
//!   - Straightforward OAuth
//!   - stateless options (or options where you handle the state) for rolling your own
//!   - all the building blocks of the convenient abstractions are available
//! - lexicon Value type for working with unknown atproto data (dag-cbor or json)
//! - order of magnitude less boilerplate than some existing crates
//! - use as much or as little from the crates as you need
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
//! ## Client options:
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
//! - `Agent<A: AgentSession>` Session abstracts over the above two options. Currently it is a thin wrapper, but this will be the thing that gets all the convenience helpers.
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
//! ## Component Crates
//!
//! Jacquard is split into several crates for modularity. The main `jacquard` crate
//! re-exports most of the others, so you typically only need to depend on it directly.
//!
//! - [`jacquard-common`] - AT Protocol types (DIDs, handles, at-URIs, NSIDs, TIDs, CIDs, etc.)
//! - [`jacquard-api`] - Generated API bindings from 646+ lexicon schemas
//! - [`jacquard-axum`] - Server-side XRPC handler extractors for Axum framework (not re-exported, depends on jacquard)
//! - [`jacquard-oauth`] - OAuth/DPoP flow implementation with session management
//! - [`jacquard-identity`] - Identity resolution (handle→DID, DID→Doc, OAuth metadata)
//! - [`jacquard-lexicon`] - Lexicon resolution, fetching, parsing and Rust code generation from schemas
//! - [`jacquard-derive`] - Macros (`#[lexicon]`, `#[open_union]`, `#[derive(IntoStatic)]`)

#![warn(missing_docs)]

pub mod client;

pub use common::*;
#[cfg(feature = "api")]
/// If enabled, re-export the generated api crate
pub use jacquard_api as api;
pub use jacquard_common as common;

#[cfg(feature = "derive")]
/// if enabled, reexport the attribute macros
pub use jacquard_derive::*;

pub use jacquard_identity as identity;

/// OAuth usage helpers (discovery, PAR, token exchange)
pub use jacquard_oauth as oauth;
