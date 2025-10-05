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
pub mod macros;
/// Baseline fundamental AT Protocol data types.
pub mod types;
