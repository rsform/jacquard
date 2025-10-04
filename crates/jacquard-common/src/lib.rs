//! Common types for the jacquard implementation of atproto

#![warn(missing_docs)]

/// A copy-on-write immutable string type that uses [`SmolStr`] for
/// the "owned" variant.
#[macro_use]
pub mod cowstr;
#[macro_use]
/// trait for taking ownership of most borrowed types in jacquard.
pub mod into_static;
/// Helper macros for common patterns
pub mod macros;
/// Baseline fundamental AT Protocol data types.
pub mod types;

pub use cowstr::CowStr;
pub use into_static::IntoStatic;
pub use smol_str;
pub use url;
