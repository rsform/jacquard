//! Build lexicon schemas from Rust AST
//!
//! This module contains shared logic for building lexicon schemas from Rust syntax trees.
//! It returns actual LexiconDoc structs (not tokens), enabling both:
//! 1. Derive macro path: doc → tokens → compiled code
//! 2. Workspace discovery path: doc directly → JSON

mod builders;
mod parse;
mod properties;
mod types;

#[cfg(test)]
mod tests;

// Re-export public API
pub use builders::{build_enum_schema, build_struct_schema};
pub use types::{
    BuiltSchema, ConstraintCheck, FieldProperty, LexiconFieldAttrs, LexiconTypeAttrs,
    LexiconTypeKind, RenameRule, SerdeAttrs, UnresolvedRef, ValidationCheck,
};

// Re-export parsing functions for derive macro
pub use parse::{
    determine_nsid, extract_option_inner, extract_variant_ref, has_open_union_attr,
    parse_field_attrs, parse_serde_attrs, parse_serde_rename_all, parse_type_attrs,
};
