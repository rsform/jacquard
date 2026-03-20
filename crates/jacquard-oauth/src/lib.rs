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

#![warn(missing_docs)]
/// AT Protocol-specific OAuth client metadata helpers and builder types.
pub mod atproto;
/// Storage trait and in-memory implementation for OAuth client auth state.
pub mod authstore;
/// High-level OAuth client for driving the full authorization code flow.
pub mod client;
/// DPoP (Demonstrating Proof-of-Possession) key generation and request signing.
pub mod dpop;
/// Top-level OAuth error types for the authorization flow.
pub mod error;
/// JOSE primitives: JWS headers, JWT claims, and signing utilities.
pub mod jose;
/// JWK keyset management for signing keys used in DPoP and client auth.
pub mod keyset;
/// Low-level OAuth request helpers: PAR, token exchange, and refresh.
pub mod request;
/// OAuth server metadata resolution: authorization server and protected resource discovery.
pub mod resolver;
///
pub mod scopes;
/// OAuth session types, token storage, and DPoP session state.
pub mod session;
/// OAuth protocol types: client metadata, token sets, and server metadata.
pub mod types;
/// Miscellaneous cryptographic utilities: key generation, PKCE, and hashing helpers.
pub mod utils;

/// Fallback signing algorithm used when no preferred algorithm is negotiated with the server.
pub const FALLBACK_ALG: &str = "ES256";

/// Loopback server helpers for the local redirect-based OAuth flow.
#[cfg(feature = "loopback")]
pub mod loopback;
