//! Core OAuth 2.1 (AT Protocol profile) types and helpers for Jacquard.
//! Transport, discovery, and orchestration live in `jacquard`.

pub mod atproto;
pub mod dpop;
pub mod error;
pub mod jose;
pub mod keyset;
pub mod resolver;
pub mod scopes;
pub mod session;
pub mod types;

pub const FALLBACK_ALG: &str = "ES256";
