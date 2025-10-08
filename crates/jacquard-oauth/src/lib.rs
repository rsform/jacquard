//! Core OAuth 2.1 (AT Protocol profile) types and helpers for Jacquard.
//! Transport, discovery, and orchestration live in `jacquard`.

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
