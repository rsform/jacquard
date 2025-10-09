//! Common types for the jacquard implementation of atproto

#![warn(missing_docs)]
pub use cowstr::CowStr;
pub use into_static::IntoStatic;
pub use smol_str;
pub use url;

/// A copy-on-write immutable string type that uses [`smol_str::SmolStr`] for
/// the "owned" variant.
#[macro_use]
pub mod cowstr;
#[macro_use]
/// Trait for taking ownership of most borrowed types in jacquard.
pub mod into_static;
pub mod error;
/// HTTP client abstraction used by jacquard crates.
pub mod http_client;
pub mod macros;
/// Generic session storage traits and utilities.
pub mod session;
/// Baseline fundamental AT Protocol data types.
pub mod types;
/// XRPC protocol types and traits
pub mod xrpc;

/// Authorization token types for XRPC requests.
#[derive(Debug, Clone)]
pub enum AuthorizationToken<'s> {
    /// Bearer token (access JWT, refresh JWT to refresh the session)
    Bearer(CowStr<'s>),
    /// DPoP token (proof-of-possession) for OAuth
    Dpop(CowStr<'s>),
}

impl<'s> IntoStatic for AuthorizationToken<'s> {
    type Output = AuthorizationToken<'static>;
    fn into_static(self) -> AuthorizationToken<'static> {
        match self {
            AuthorizationToken::Bearer(token) => AuthorizationToken::Bearer(token.into_static()),
            AuthorizationToken::Dpop(token) => AuthorizationToken::Dpop(token.into_static()),
        }
    }
}
