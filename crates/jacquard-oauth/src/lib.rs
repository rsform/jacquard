//! # Jacquard OAuth 2.1 implementation for the AT Protocol
//!
//! Implements the AT Protocol OAuth profile, including DPoP (Demonstrating
//! Proof-of-Possession), PKCE, PAR (Pushed Authorization Requests), and token management.
//!
//!
//! ## Authentication flow
//!
//! ```no_run
//! # #[cfg(feature = "loopback")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use jacquard_oauth::client::OAuthClient;
//! use jacquard_oauth::session::ClientData;
//! use jacquard_oauth::atproto::AtprotoClientMetadata;
//! use jacquard_oauth::loopback::LoopbackConfig;
//! use jacquard_oauth::authstore::MemoryAuthStore;
//!
//! let store = MemoryAuthStore::new();
//!
//! // Create client with metadata
//! let client_data = ClientData {
//!     keyset: None,  // Will generate ES256 keypair if needed
//!     config: AtprotoClientMetadata::default_localhost(),
//! };
//! let oauth = OAuthClient::new(store, client_data);
//!
//! // Start auth flow (with loopback feature)
//! let session = oauth.login_with_local_server(
//!     "alice.bsky.social",
//!     Default::default(),
//!     LoopbackConfig::default(),
//! ).await?;
//!
//! // Session handles token refresh automatically
//! # Ok(())
//! # }
//! ```
//!
//! ## AT Protocol specifics
//!
//! The AT Protocol OAuth profile adds:
//! - Required DPoP for all token requests
//! - PAR (Pushed Authorization Requests) for better security
//! - Specific scope format (`atproto`, `transition:generic`, etc.)
//! - Server metadata discovery at `/.well-known/oauth-authorization-server`
//!
//! See [`atproto`] module for AT Protocol-specific metadata helpers.

pub mod atproto;
pub mod authstore;
pub mod client;
pub mod dpop;
pub mod error;
pub mod jose;
pub mod keyset;
pub mod request;
pub mod resolver;
pub mod scopes;
pub mod session;
pub mod types;
pub mod utils;

pub const FALLBACK_ALG: &str = "ES256";

#[cfg(feature = "loopback")]
pub mod loopback;
