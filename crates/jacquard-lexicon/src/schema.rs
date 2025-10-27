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
//! ### Derive Macro
//!
//! Use `#[derive(LexiconSchema)]` to automatically generate schemas:
//!
//! ```rust,ignore
//! use jacquard_lexicon::schema::LexiconSchema;
//! use jacquard_common::CowStr;
//!
//! #[derive(LexiconSchema)]
//! #[lexicon(nsid = "app.bsky.feed.post", record, key = "tid")]
//! struct Post<'a> {
//!     #[lexicon(max_graphemes = 300, max_length = 3000)]
//!     text: CowStr<'a>,
//!     created_at: Datetime,
//! }
//! ```
//!
//! #### Constraint Attributes
//!
//! - **Field constraints**: `max_length`, `max_graphemes`, `min_length`, `min_graphemes`
//! - **Array constraints**: `max_items`, `min_items` (for the array itself)
//! - **Item constraints**: `item_max_length`, `item_max_graphemes`, etc. (for array items)
//! - **Integer constraints**: `minimum`, `maximum`
//! - **Refs**: `ref = "nsid"` to explicitly reference another type
//! - **Unions**: `union` to mark a field as a union type
//!
//! #### Fragments
//!
//! Multiple types can share the same NSID using fragments:
//!
//! ```rust,ignore
//! #[derive(LexiconSchema)]
//! #[lexicon(nsid = "app.bsky.feed.post", fragment = "textSlice")]
//! struct TextSlice {
//!     start: i64,
//!     end: i64,
//! }
//! ```
//!
//! ### Runtime Registry
//!
//! Access complete schemas (with all fragments merged) via the global registry:
//!
//! ```rust,ignore
//! let registry = jacquard_lexicon::schema::global_registry();
//! let post_doc = registry.get("app.bsky.feed.post").expect("schema exists");
//!
//! // The doc contains all defs: main, textSlice, entity, replyRef, etc.
//! for (def_name, def) in &post_doc.defs {
//!     println!("Def: {}", def_name);
//! }
//! ```
//!
//! ## Design Pattern
//!
//! - **Trait-based**: Types implement `LexiconSchema` trait
//! - **Inventory-based discovery**: Runtime schema registry via `inventory` crate
//! - **Fragment merging**: Multiple types with same NSID have their defs merged
//! - **Const literals**: Generated code emits schema as const data
//! - **Validation**: Runtime constraint checking via `validate()` method

pub mod builder;
#[cfg(feature = "codegen")]
pub mod from_ast;
#[cfg(feature = "codegen")]
pub mod type_mapping;

use crate::lexicon::LexiconDoc;

/// Trait for types that can generate lexicon schemas
pub trait LexiconSchema {
    /// The NSID for this type's primary definition
    ///
    /// For fragments, this is the base NSID (without `#fragment`).
    fn nsid() -> &'static str;

    /// The definition name within the lexicon document
    ///
    /// Returns "main" for the primary definition, or the fragment name for other defs.
    /// For example, in a lexicon with multiple defs like `pub.leaflet.poll.definition`,
    /// the main type returns "main" while the `Option` type returns "option".
    fn def_name() -> &'static str {
        "main"
    }

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
    fn validate(&self) -> Result<(), crate::validation::ConstraintError> {
        // Default impl: no constraints to check
        Ok(())
    }
}

/// Registry entry for schema discovery via inventory
///
/// Generated automatically by `#[derive(LexiconSchema)]` to enable runtime schema discovery.
pub struct LexiconSchemaRef {
    /// The NSID for this schema
    pub nsid: &'static str,
    /// The def name within the lexicon (e.g., "main", "textSlice")
    pub def_name: &'static str,
    /// Function that generates the lexicon document
    pub provider: fn() -> crate::lexicon::LexiconDoc<'static>,
}

inventory::collect!(LexiconSchemaRef);

/// Registry of lexicon schemas
///
/// Collects schemas from inventory at construction and supports runtime insertion.
#[derive(Debug, Clone)]
pub struct SchemaRegistry {
    /// Schema documents indexed by NSID (concurrent access safe)
    schemas: dashmap::DashMap<jacquard_common::smol_str::SmolStr, crate::lexicon::LexiconDoc<'static>>,
}

impl SchemaRegistry {
    /// Build registry from inventory-collected schemas
    pub fn from_inventory() -> Self {
        use jacquard_common::smol_str::ToSmolStr;
        let schemas = dashmap::DashMap::new();

        for entry in inventory::iter::<LexiconSchemaRef> {
            let doc = (entry.provider)();

            // Get existing doc or create new one
            let mut doc_entry = schemas.entry(entry.nsid.to_smolstr()).or_insert_with(|| {
                crate::lexicon::LexiconDoc {
                    lexicon: crate::lexicon::Lexicon::Lexicon1,
                    id: jacquard_common::CowStr::new_static(entry.nsid),
                    revision: None,
                    description: None,
                    defs: Default::default(),
                }
            });

            // Merge the defs from this schema
            // Each type's lexicon_doc() now returns a doc with the def under its proper name
            for (def_name, def) in doc.defs {
                doc_entry.defs.insert(def_name, def);
            }
        }

        Self { schemas }
    }

    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            schemas: dashmap::DashMap::new(),
        }
    }

    /// Get schema by NSID
    ///
    /// IMPORTANT: Clone the returned schema immediately to avoid holding DashMap ref
    pub fn get(&self, nsid: &str) -> Option<crate::lexicon::LexiconDoc<'static>> {
        self.schemas.get(nsid).map(|doc| doc.clone())
    }

    /// Insert or update a schema (for runtime schema loading)
    pub fn insert(&self, nsid: jacquard_common::smol_str::SmolStr, doc: crate::lexicon::LexiconDoc<'static>) {
        self.schemas.insert(nsid, doc);
    }

    /// Get specific def from a schema
    ///
    /// IMPORTANT: Returns cloned def to avoid holding DashMap ref
    pub fn get_def(
        &self,
        nsid: &str,
        def_name: &str,
    ) -> Option<crate::lexicon::LexUserType<'static>> {
        // Clone immediately to release DashMap ref before returning
        self.schemas
            .get(nsid)
            .and_then(|doc| doc.defs.get(def_name).cloned())
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::from_inventory()
    }
}

/// Global schema registry built from inventory
pub fn global_registry() -> &'static SchemaRegistry {
    static REGISTRY: std::sync::LazyLock<SchemaRegistry> = std::sync::LazyLock::new(SchemaRegistry::from_inventory);
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use crate::validation::{ConstraintError, ValidationPath};

    #[test]
    fn test_validation_max_length() {
        let err = ConstraintError::MaxLength {
            path: ValidationPath::from_field("text"),
            max: 100,
            actual: 150,
        };
        assert!(err.to_string().contains("exceeds max length"));
    }

    #[test]
    fn test_validation_max_graphemes() {
        let err = ConstraintError::MaxGraphemes {
            path: ValidationPath::from_field("text"),
            max: 50,
            actual: 75,
        };
        assert!(err.to_string().contains("exceeds max graphemes"));
    }
}
