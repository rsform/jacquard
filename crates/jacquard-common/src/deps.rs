//! Re-exports of external crate dependencies for consistent access across jacquard.
//!
//! This module centralises all external crate access so downstream crates and generated code
//! reference known-good versions through a single path. This allows for easier dependency
//! management and version upgrades across the workspace.

/// Re-export of the [`bytes`](https://docs.rs/bytes/) crate for byte buffer operations.
pub use bytes;

/// Re-export of the [`chrono`](https://docs.rs/chrono/) crate for date/time handling.
pub use chrono;

/// Re-export of the [`smol_str`](https://docs.rs/smol_str/) crate for optimized small strings.
pub use smol_str;

/// Re-export of the [`fluent_uri`](https://docs.rs/fluent-uri/) crate for RFC 3986 compliant URI parsing and manipulation.
pub use fluent_uri;

/// Re-exports of crates used in code generation contexts.
///
/// This submodule is hidden from documentation and intended for use by generated code
/// and related internal tooling. Stability guarantees may differ from the public API.
#[doc(hidden)]
pub mod codegen {
    /// Re-export of the [`serde`](https://docs.rs/serde/) crate for serialization/deserialization.
    pub use serde;

    /// Re-export of the [`serde_bytes`](https://docs.rs/serde_bytes/) crate for efficient byte serialization.
    pub use serde_bytes;

    /// Re-export of the [`serde_ipld_dagcbor`](https://docs.rs/serde_ipld_dagcbor/) crate for CBOR encoding.
    pub use serde_ipld_dagcbor;

    /// Re-export of the [`miette`](https://docs.rs/miette/) crate for rich error diagnostics.
    /// Only available when the `std` feature is enabled.
    #[cfg(feature = "std")]
    pub use miette;

    /// Re-export of the [`thiserror`](https://docs.rs/thiserror/) crate for error definition macros.
    pub use thiserror;

    /// Re-export of the [`rustversion`](https://docs.rs/rustversion/) crate for conditional compilation based on Rust version.
    pub use rustversion;

    /// Re-export of the [`unicode_segmentation`](https://docs.rs/unicode-segmentation/) crate for grapheme counting.
    pub use unicode_segmentation;
}
