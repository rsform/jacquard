//! # Lexicon Schema Generation
//!
//! This module provides traits and utilities for generating AT Protocol lexicon schemas
//! from Rust types. This is the reverse direction from the usual lexicon→Rust codegen.
//!
//! ## Use Cases
//!
//! - **Rapid prototyping**: Define types in Rust, generate schemas automatically
//! - **Custom lexicons**: Easy creation of third-party lexicons
//! - **Documentation**: Keep types and schemas in sync
//! - **Runtime introspection**: Access schema metadata at runtime
//!
//! ## Usage
//!
//! ### Manual Implementation
//!
//! Implement the `LexiconSchema` trait for your types:
//!
//! ```rust
//! # use jacquard_lexicon::schema::LexiconSchema;
//! # use jacquard_lexicon::lexicon::LexiconDoc;
//! struct MyType;
//!
//! impl LexiconSchema for MyType {
//!     fn nsid() -> &'static str {
//!         "com.example.myType"
//!     }
//!
//!     fn lexicon_doc() -> LexiconDoc<'static> {
//!         // Build schema using builder API or const literals
//!         todo!()
//!     }
//! }
//! ```
//!
//! ### Derive Macro (Future)
//!
//! In Phase 2, a derive macro will automate trait implementation:
//!
//! ```rust,ignore
//! #[derive(LexiconSchema)]
//! #[lexicon(nsid = "app.bsky.feed.post", record, key = "tid")]
//! struct Post<'a> {
//!     #[lexicon(max_graphemes = 300, max_length = 3000)]
//!     text: CowStr<'a>,
//!     created_at: Datetime,
//! }
//! ```
//!
//! ## Design Pattern
//!
//! - **Trait-based**: Types implement `LexiconSchema` trait
//! - **Inventory-based discovery**: Runtime schema registry via `inventory` crate
//! - **Const literals**: Generated code emits schema as const data
//! - **Validation**: Runtime constraint checking via `validate()` method

pub mod builder;
pub mod from_ast;
pub mod type_mapping;

use crate::lexicon::LexiconDoc;

/// Trait for types that can generate lexicon schemas
pub trait LexiconSchema {
    /// The NSID for this type's primary definition
    ///
    /// For fragments, this is the base NSID (without `#fragment`).
    fn nsid() -> &'static str;

    /// The schema ID for this type
    ///
    /// Defaults to NSID. Override for fragments to include `#fragment` suffix.
    fn schema_id() -> jacquard_common::CowStr<'static> {
        jacquard_common::CowStr::new_static(Self::nsid())
    }

    /// Whether this type should be inlined vs referenced
    ///
    /// - `false` (default): Type becomes a def, references use `{"type": "ref", "ref": "nsid"}`
    /// - `true`: Type's schema is inlined directly into parent
    ///
    /// Recursive types MUST return `false` to avoid infinite expansion.
    fn inline_schema() -> bool {
        false
    }

    /// Generate the lexicon document for this type
    ///
    /// Returns the complete lexicon schema for this type. Nested refs are resolved
    /// at runtime via the inventory-based registry.
    fn lexicon_doc() -> LexiconDoc<'static>;

    /// Validate an instance against lexicon constraints
    ///
    /// Checks runtime constraints like `max_length`, `max_graphemes`, `minimum`, etc.
    /// Returns `Ok(())` if valid, `Err` with details if invalid.
    fn validate(&self) -> Result<(), ValidationError> {
        // Default impl: no constraints to check
        Ok(())
    }
}

/// Error type for validation failures
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic)]
pub enum ValidationError {
    #[error("field `{field}` exceeds maximum length: {actual} > {max}")]
    MaxLength {
        field: &'static str,
        max: usize,
        actual: usize,
    },

    #[error("field `{field}` exceeds maximum grapheme count: {actual} > {max}")]
    MaxGraphemes {
        field: &'static str,
        max: usize,
        actual: usize,
    },

    #[error("field `{field}` below minimum length: {actual} < {min}")]
    MinLength {
        field: &'static str,
        min: usize,
        actual: usize,
    },

    #[error("field `{field}` below minimum grapheme count: {actual} < {min}")]
    MinGraphemes {
        field: &'static str,
        min: usize,
        actual: usize,
    },

    #[error("field `{field}` value {actual} exceeds maximum: {max}")]
    Maximum {
        field: &'static str,
        max: i64,
        actual: i64,
    },

    #[error("field `{field}` value {actual} below minimum: {min}")]
    Minimum {
        field: &'static str,
        min: i64,
        actual: i64,
    },

    #[error("field `{field}` has invalid value: {message}")]
    InvalidValue {
        field: &'static str,
        message: String,
    },

    #[error("multiple validation errors: {0:?}")]
    Multiple(Vec<ValidationError>),
}

/// Registry entry for schema discovery via inventory
///
/// Generated automatically by `#[derive(LexiconSchema)]` to enable runtime schema discovery.
/// Phase 3 will use this to extract all schemas from a binary.
pub struct LexiconSchemaRef {
    /// The NSID for this schema
    pub nsid: &'static str,
    /// Function that generates the lexicon document
    pub provider: fn() -> crate::lexicon::LexiconDoc<'static>,
}

inventory::collect!(LexiconSchemaRef);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_max_length() {
        let err = ValidationError::MaxLength {
            field: "text",
            max: 100,
            actual: 150,
        };
        assert!(err.to_string().contains("exceeds maximum length"));
    }

    #[test]
    fn test_validation_max_graphemes() {
        let err = ValidationError::MaxGraphemes {
            field: "text",
            max: 50,
            actual: 75,
        };
        assert!(err.to_string().contains("exceeds maximum grapheme count"));
    }
}
