#![doc = include_str!("../../../README.md")]
#![warn(missing_docs)]

/// XRPC client traits and basic implementation
pub mod client;

#[cfg(feature = "api")]
/// If enabled, re-export the generated api crate
pub use jacquard_api as api;
/// Re-export common types
pub use jacquard_common::*;

#[cfg(feature = "derive")]
/// if enabled, reexport the attribute macros
pub use jacquard_derive::*;
