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
//! Dead simple api client. Logs in, prints the latest 5 posts from your timeline.
//!
//! ```no_run
//! # use clap::Parser;
//! # use jacquard::CowStr;
//! use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
//! use jacquard::api::com_atproto::server::create_session::CreateSession;
//! use jacquard::client::{BasicClient, Session};
//! # use miette::IntoDiagnostic;
//!
//! # #[derive(Parser, Debug)]
//! # #[command(author, version, about = "Jacquard - AT Protocol client demo")]
//! # struct Args {
//! #     /// Username/handle (e.g., alice.mosphere.at)
//! #     #[arg(short, long)]
//! #     username: CowStr<'static>,
//! #
//! #     /// PDS URL (e.g., https://bsky.social)
//! #     #[arg(long, default_value = "https://bsky.social")]
//! #     pds: CowStr<'static>,
//! #
//! #     /// App password
//! #     #[arg(short, long)]
//! #     password: CowStr<'static>,
//! # }
//!
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let args = Args::parse();
//!
//!     // Create HTTP client
//!     let url = url::Url::parse(&args.pds).unwrap();
//!     let client = BasicClient::new(url);
//!
//!     // Create session
//!     let session = Session::from(
//!         client
//!             .send(
//!                 CreateSession::new()
//!                     .identifier(args.username)
//!                     .password(args.password)
//!                     .build(),
//!             )
//!             .await?
//!             .into_output()?,
//!     );
//!
//!     println!("logged in as {} ({})", session.handle, session.did);
//!     client.set_session(session).await.unwrap();
//!
//!     // Fetch timeline
//!     println!("\nfetching timeline...");
//!     let timeline = client
//!         .send(GetTimeline::new().limit(5).build())
//!         .await?
//!         .into_output()?;
//!
//!     println!("\ntimeline ({} posts):", timeline.feed.len());
//!     for (i, post) in timeline.feed.iter().enumerate() {
//!         println!("\n{}. by {}", i + 1, post.post.author.handle);
//!         println!(
//!             "   {}",
//!             serde_json::to_string_pretty(&post.post.record).into_diagnostic()?
//!        );
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Clients
//!
//! - Stateless XRPC: any `HttpClient` (e.g., `reqwest::Client`) implements `XrpcExt`,
//!   which provides `xrpc(base: Url) -> XrpcCall` for per-request calls with
//!   optional `CallOptions` (auth, proxy, labelers, headers). Useful when you
//!   want to pass auth on each call or build advanced flows.
//!   Example
//!   ```ignore
//!   use jacquard::client::XrpcExt;
//!   use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
//!   use jacquard::types::ident::AtIdentifier;
//!
//!   #[tokio::main]
//!   async fn main() -> anyhow::Result<()> {
//!       let http = reqwest::Client::new();
//!       let base = url::Url::parse("https://public.api.bsky.app")?;
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
//! - Stateful client: `AtClient<C, S>` holds a base `Url`, a transport, and a
//!   `TokenStore` implementation. It automatically sets Authorization and can
//!   auto-refresh a session when expired, retrying once.
//! - Convenience wrapper: `BasicClient` is an ergonomic newtype over
//!   `AtClient<reqwest::Client, MemoryTokenStore>` with a `new(Url)` constructor.
//!
//! Per-request overrides (stateless)
//! ```ignore
//! use jacquard::client::{XrpcExt, AuthorizationToken};
//! use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;
//! use jacquard::types::ident::AtIdentifier;
//! use jacquard::CowStr;
//! use miette::IntoDiagnostic;
//!
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     let http = reqwest::Client::new();
//!     let base = url::Url::parse("https://public.api.bsky.app")?;
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
//! - Use `MemoryTokenStore` for ephemeral sessions, tests, and CLIs.
//! - For persistence, `FileTokenStore` stores session tokens as JSON on disk.
//!   See `client::token::FileTokenStore` docs for details.
//!   Example
//!   ```ignore
//!   use jacquard::client::{AtClient, FileTokenStore};
//!   let base = url::Url::parse("https://bsky.social").unwrap();
//!   let store = FileTokenStore::new("/tmp/jacquard-session.json");
//!   let client = AtClient::new(reqwest::Client::new(), base, store);
//!   ```
//!

#![warn(missing_docs)]

/// XRPC client traits and basic implementation
pub mod client;

#[cfg(feature = "api")]
/// If enabled, re-export the generated api crate
pub use jacquard_api as api;
pub use jacquard_common::*;

#[cfg(feature = "derive")]
/// if enabled, reexport the attribute macros
pub use jacquard_derive::*;

/// Identity resolution helpers (DIDs, handles, PDS endpoints)
pub mod identity;
