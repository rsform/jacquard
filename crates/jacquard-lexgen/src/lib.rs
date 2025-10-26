//! # Lexicon fetching and code generation binaries for Jacquard
//!
//! This crate provides the tooling for fetching lexicon schemas from various sources
//! and generating Rust code from them.
//!
//! ## Binaries
//!
//! ### lex-fetch
//!
//! Downloads lexicons from configured sources and runs the code generation pipeline:
//!
//! ```bash
//! cargo run -p jacquard-lexgen --bin lex-fetch
//! ```
//!
//! Configuration lives in `lexicons.kdl` at the workspace root.
//!
//! ### jacquard-codegen
//!
//! Runs code generation on a local directory of lexicons:
//!
//! ```bash
//! cargo run -p jacquard-lexgen --bin jacquard-codegen -- \
//!     -i ./lexicons \
//!     -o ./crates/jacquard-api/src
//! ```
//!
//! ## Modules
//!
//! - [`fetch`] - Ingests lexicons from git, atproto, http fetch, and other sources
//! - [`cli`] - CLI argument parsing utilities
//! - [`schema_extraction`] - Extract lexicon schemas from Rust types via inventory (link-time discovery)
//! - [`schema_discovery`] - Discover schemas by scanning workspace source files (no linking required)

pub mod cli;
pub mod fetch;
pub mod schema_discovery;
pub mod schema_extraction;
#[cfg(any(test, debug_assertions))]
pub mod test_schemas;

pub use fetch::{Config, Fetcher};
