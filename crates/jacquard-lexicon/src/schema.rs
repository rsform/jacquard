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
//! # use jacquard_lexicon::schema::{LexiconSchema, LexiconGenerator};
//! # use jacquard_lexicon::lexicon::LexiconDoc;
//! struct MyType;
//!
//! impl LexiconSchema for MyType {
//!     fn nsid() -> &'static str {
//!         "com.example.myType"
//!     }
//!
//!     fn lexicon_doc(generator: &mut LexiconGenerator) -> LexiconDoc<'static> {
//!         // Build schema using generator or builder API
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
//! The design follows `schemars` crate patterns:
//!
//! - **Trait-based**: Types implement `LexiconSchema` trait
//! - **Generator pattern**: `LexiconGenerator` tracks refs/defs
//! - **Inline vs refs**: Types control whether they inline or use refs
//! - **Validation**: Runtime constraint checking via `validate()` method

pub mod builder;
pub mod type_mapping;

use crate::lexicon::{LexObjectProperty, LexRef, LexUserType, Lexicon, LexiconDoc};
use jacquard_common::smol_str::SmolStr;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

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
    /// Called by generator to build complete schema. Use `generator` to handle
    /// nested types and refs.
    fn lexicon_doc(generator: &mut LexiconGenerator) -> LexiconDoc<'static>;

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

/// Generator for lexicon schemas
///
/// Tracks seen types and manages refs vs inline decisions.
/// Similar pattern to `schemars::SchemaGenerator`.
pub struct LexiconGenerator {
    /// Root NSID for the primary type being generated
    root_nsid: SmolStr,

    /// Collected definitions (def_name -> LexUserType)
    defs: BTreeMap<SmolStr, LexUserType<'static>>,

    /// Types we've seen (prevents duplicate defs)
    seen_types: HashSet<SmolStr>,

    /// Stack of types currently being generated (cycle detection)
    generation_stack: Vec<SmolStr>,
}

impl LexiconGenerator {
    /// Create a new generator for a primary type
    pub fn new(root_nsid: impl Into<SmolStr>) -> Self {
        Self {
            root_nsid: root_nsid.into(),
            defs: BTreeMap::new(),
            seen_types: HashSet::new(),
            generation_stack: Vec::new(),
        }
    }

    /// Generate schema for a type that implements LexiconSchema
    ///
    /// Returns either a ref property or an inline object property depending on `inline_schema()`.
    /// Also adds any defs to the generator's collection.
    pub fn generate_for<T: LexiconSchema>(
        &mut self,
    ) -> Result<LexObjectProperty<'static>, GeneratorError> {
        let schema_id = T::schema_id();

        // Check for cycles
        if self.generation_stack.contains(&schema_id.as_ref().into()) {
            return Err(GeneratorError::RecursiveType {
                type_name: schema_id.to_string(),
                stack: self.generation_stack.clone(),
            });
        }

        // If we've seen this type and it's not inline, return a ref
        if !T::inline_schema() && self.seen_types.contains(schema_id.as_ref()) {
            return Ok(self.make_ref_property(schema_id.as_ref()));
        }

        // Mark as seen and add to stack
        self.seen_types.insert(schema_id.to_string().into());
        self.generation_stack.push(schema_id.to_string().into());

        // Generate the schema
        let doc = T::lexicon_doc(self);

        // Pop from stack
        self.generation_stack.pop();

        // If inline, extract the main def and convert to property
        if T::inline_schema() {
            // Find the main def and convert to property type
            if let Some(def) = doc.defs.get("main") {
                // Convert LexUserType to LexObjectProperty
                match def {
                    LexUserType::Object(obj) => Ok(LexObjectProperty::Object(obj.clone())),
                    LexUserType::String(s) => Ok(LexObjectProperty::String(s.clone())),
                    LexUserType::Array(a) => Ok(LexObjectProperty::Array(a.clone())),
                    LexUserType::Boolean(b) => Ok(LexObjectProperty::Boolean(b.clone())),
                    LexUserType::Integer(i) => Ok(LexObjectProperty::Integer(i.clone())),
                    LexUserType::Blob(b) => Ok(LexObjectProperty::Blob(b.clone())),
                    LexUserType::Bytes(b) => Ok(LexObjectProperty::Bytes(b.clone())),
                    LexUserType::CidLink(c) => Ok(LexObjectProperty::CidLink(c.clone())),
                    LexUserType::Unknown(u) => Ok(LexObjectProperty::Unknown(u.clone())),
                    // Types that cannot be inlined as properties
                    _ => Err(GeneratorError::InvalidInlineType {
                        type_name: schema_id.to_string(),
                        user_type: format!("{:?}", def),
                    }),
                }
            } else {
                Err(GeneratorError::EmptyDoc {
                    type_name: schema_id.to_string(),
                })
            }
        } else {
            // Not inline - merge defs into our collection and return ref
            for (def_name, def) in doc.defs {
                self.defs.insert(def_name, def);
            }
            Ok(self.make_ref_property(schema_id.as_ref()))
        }
    }

    /// Create a ref property to another type
    ///
    /// Returns a LexObjectProperty::Ref for use in object properties or array items.
    pub fn make_ref_property(&self, ref_nsid: &str) -> LexObjectProperty<'static> {
        LexObjectProperty::Ref(LexRef {
            description: None,
            r#ref: ref_nsid.to_string().into(),
        })
    }

    /// Build the final lexicon document
    pub fn into_doc(self) -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: self.root_nsid.into(),
            revision: None,
            description: None,
            defs: self.defs,
        }
    }

    /// Add a def directly (for manual construction)
    pub fn add_def(&mut self, name: impl Into<SmolStr>, def: LexUserType<'static>) {
        self.defs.insert(name.into(), def);
    }

    /// Get the root NSID
    pub fn root_nsid(&self) -> &str {
        &self.root_nsid
    }
}

/// Errors from lexicon generation
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic)]
pub enum GeneratorError {
    #[error("recursive type detected: {type_name}")]
    RecursiveType {
        type_name: String,
        stack: Vec<SmolStr>,
    },

    #[error("type {type_name} generated empty document")]
    EmptyDoc { type_name: String },

    #[error("type {type_name} marked as inline but main def type cannot be inlined: {user_type}")]
    InvalidInlineType {
        type_name: String,
        user_type: String,
    },

    #[error("invalid NSID: {nsid}")]
    InvalidNsid { nsid: String },
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
    use crate::lexicon::{
        LexBoolean, LexInteger, LexObject, LexRecord, LexRecordRecord, LexString,
    };

    #[test]
    fn test_generator_simple() {
        let mut generator = LexiconGenerator::new("app.example.test");

        // Add a simple record
        generator.add_def(
            "main",
            LexUserType::Record(LexRecord {
                description: Some("Test record".into()),
                key: Some("tid".into()),
                record: LexRecordRecord::Object(LexObject {
                    description: None,
                    required: Some(vec!["field1".into()]),
                    nullable: None,
                    properties: [(
                        "field1".into(),
                        LexObjectProperty::String(LexString {
                            description: None,
                            format: None,
                            default: None,
                            min_length: None,
                            max_length: None,
                            min_graphemes: None,
                            max_graphemes: None,
                            r#enum: None,
                            r#const: None,
                            known_values: None,
                        }),
                    )]
                    .into(),
                }),
            }),
        );

        let doc = generator.into_doc();
        assert_eq!(doc.id.as_ref(), "app.example.test");
        assert_eq!(doc.defs.len(), 1);
        assert!(doc.defs.contains_key("main"));
    }

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
