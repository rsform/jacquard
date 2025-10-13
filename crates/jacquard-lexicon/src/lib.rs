//! # Lexicon schema parsing and Rust code generation for the Jacquard atproto ecosystem
//!
//! This crate also provides lexicon fetching capabilitiees ofr
//!
//! ## Usage
//!
//! ### Fetch lexicons
//!
//! The `lex-fetch` binary downloads lexicons from configured sources and
//! runs the code generation pipeline on them:
//!
//! ```bash
//! cargo run -p jacquard-lexicon --bin lex-fetch
//! ```
//!
//! Configuration lives in `lexicons.kdl` at the workspace root.
//!
//! ### Generate Rust code
//!
//! The `jacquard-codegen` binary can be pointed at a local directory to
//! runs the code generation pipeline:
//!
//! ```bash
//! cargo run -p jacquard-lexicon --bin jacquard-codegen -- \
//!     -i ./lexicons \
//!     -o ./crates/jacquard-api/src
//! ```
//!
//!
//! ## Modules
//!
//! - [`codegen`] - Rust code generation from parsed schemas
//! - [`corpus`] - Lexicon corpus management and namespace organization
//! - [`lexicon`] - Schema parsing and validation
//! - [`union_registry`] - Tracks union types for collision detection
//! - [`fetch`] - Ingests lexicons from git, atproto, http fetch, and other sources
//! - [`fs`] - Filesystem utilities for lexicon storage

pub mod codegen;
pub mod corpus;
pub mod error;
pub mod fetch;
pub mod fs;
pub mod lexicon;
pub mod union_registry;
