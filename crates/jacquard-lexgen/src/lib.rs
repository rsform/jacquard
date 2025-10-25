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

pub mod cli;
pub mod fetch;

pub use fetch::{Config, Fetcher};
