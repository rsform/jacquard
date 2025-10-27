//! # Lexicon schema parsing and Rust code generation for the Jacquard atproto ecosystem
//!
//! This crate provides the core lexicon types and code generation infrastructure.
//!
//! For fetching lexicons and running the codegen binaries, see the `jacquard-lexgen` crate.
//!
//! ## Modules
//!
//! - [`codegen`] - Rust code generation from parsed schemas
//! - [`corpus`] - Lexicon corpus management and namespace organization
//! - [`lexicon`] - Schema parsing and validation
//! - [`schema`] - Schema generation from Rust types (reverse codegen)
//! - [`union_registry`] - Tracks union types for collision detection
//! - [`fs`] - Filesystem utilities for lexicon storage
//! - [`derive_impl`] - Implementation functions for derive macros (used by jacquard-derive)
//! - [`validation`] - Runtime validation of Data against lexicon schemas

#[cfg(feature = "codegen")]
pub mod codegen;
#[cfg(feature = "codegen")]
pub mod corpus;
#[cfg(feature = "codegen")]
#[doc(hidden)]
pub mod derive_impl;
#[cfg(feature = "codegen")]
pub mod error;
#[cfg(feature = "codegen")]
pub mod fs;
pub mod lexicon;
pub mod ref_utils;
pub mod schema;
pub mod validation;
