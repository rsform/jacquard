//! # Jacquard
//!
//! A suite of Rust crates for the AT Protocol.
//!
//!
//! ## Goals
//!
//! - Validated, spec-compliant, easy to work with, and performant baseline types (including typed at:// uris)
//! - Batteries-included, but easily replaceable batteries.
//!   - Easy to extend with custom lexicons
//! - lexicon Value type for working with unknown atproto data (dag-cbor or json)
//! - order of magnitude less boilerplate than some existing crates
//!   - either the codegen produces code that's easy to work with, or there are good handwritten wrappers
//! - didDoc type with helper methods for getting handles, multikey, and PDS endpoint
//! - use as much or as little from the crates as you need
//!
//!
//! ## Example
//!
//! Dead simple API client: login with an app password, then fetch the latest 5 posts.
//!
//! ```no_run
//! # use clap::Parser;
//! # use jacquard::CowStr;
//! use std::sync::Arc;
//! use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
//! use jacquard::client::credential_session::{CredentialSession, SessionKey};
//! use jacquard::client::{AtpSession, FileAuthStore, MemorySessionStore};
//! use jacquard::identity::PublicResolver as JacquardResolver;
//! # use miette::IntoDiagnostic;
//!
//! # #[derive(Parser, Debug)]
//! # #[command(author, version, about = "Jacquard - AT Protocol client demo")]
//! # struct Args {
//! #     /// Username/handle (e.g., alice.bsky.social) or DID
//! #     #[arg(short, long)]
//! #     username: CowStr<'static>,
//! #
//! #     /// App password
//! #     #[arg(short, long)]
//! #     password: CowStr<'static>,
//! # }
//!
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let args = Args::parse();
//!     // Resolver + storage
//!     let resolver = Arc::new(JacquardResolver::default());
//!     let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
//!     let client = Arc::new(resolver.clone());
//!     // Create session object with implicit public appview endpoint until login/restore
//!     let session = CredentialSession::new(store, client);
//!     // Log in (resolves PDS automatically) and persist as (did, "session")
//!     session
//!         .login(args.username.clone(), args.password.clone(), None, None, None)
//!         .await
//!         .into_diagnostic()?;
//!     // Fetch timeline
//!     let timeline = session
//!         .clone()
//!         .send(GetTimeline::new().limit(5).build())
//!         .await
//!         .into_diagnostic()?
//!         .into_output()
//!         .into_diagnostic()?;
//!     println!("timeline ({} posts):", timeline.feed.len());
//!     for (i, post) in timeline.feed.iter().enumerate() {
//!         println!("{}. by {}", i + 1, post.post.author.handle);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Client options:
//!
//! - Stateless XRPC: any `HttpClient` (e.g., `reqwest::Client`) implements `XrpcExt`,
//!   which provides `xrpc(base: Url) -> XrpcCall` for per-request calls with
//!   optional `CallOptions` (auth, proxy, labelers, headers). Useful when you
//!   want to pass auth on each call or build advanced flows.
//!  ```no_run
//!   #  use jacquard::types::xrpc::XrpcExt;
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
//!               GetAuthorFeed::new()
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
//!   `T: IdentityResolver + HttpClient + XrpcExt`. It auto-attaches Authorization, refreshes on expiry, and updates the
//!   base endpoint to the user's PDS on login/restore.
//!
//! Per-request overrides (stateless)
//! ```no_run
//! # use jacquard::AuthorizationToken;
//! # use jacquard::types::xrpc::XrpcExt;
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
//!             GetAuthorFeed::new()
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
//! Token storage:
//! - Use `MemorySessionStore<SessionKey, AtpSession>` for ephemeral sessions and tests.
//! - For persistence, wrap the file store: `FileAuthStore::new(path)` implements SessionStore for app-password sessions
//!   and OAuth `ClientAuthStore` (unified on-disk map).
//!   ```no_run
//!   use std::sync::Arc;
//!   use jacquard::client::credential_session::{CredentialSession, SessionKey};
//!   use jacquard::client::{AtpSession, FileAuthStore};
//!   use jacquard::identity::PublicResolver;
//!   let store = Arc::new(FileAuthStore::new("/tmp/jacquard-session.json"));
//!   let client = Arc::new(PublicResolver::default());
//!   let session = CredentialSession::new(store, client);
//!   ```
//!

#![warn(missing_docs)]

/// XRPC client traits and basic implementation
pub mod client;
/// OAuth usage helpers (discovery, PAR, token exchange)

#[cfg(feature = "api")]
/// If enabled, re-export the generated api crate
pub use jacquard_api as api;
pub use jacquard_common::*;

#[cfg(feature = "derive")]
/// if enabled, reexport the attribute macros
pub use jacquard_derive::*;

pub use jacquard_identity as identity;
