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
//! ```rust
//! # use clap::Parser;
//! # use jacquard::CowStr;
//! use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
//! use jacquard::api::com_atproto::server::create_session::CreateSession;
//! use jacquard::client::{AuthenticatedClient, Session, XrpcClient};
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
//!     let mut client = AuthenticatedClient::new(reqwest::Client::new(), args.pds);
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
//!     client.set_session(session);
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
