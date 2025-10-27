//! Type definitions for schema building

use crate::lexicon::*;
use syn::Type;

/// Result of building a schema from AST
#[derive(Debug, Clone)]
pub struct BuiltSchema {
    /// Base NSID (without fragment)
    pub nsid: String,
    /// Full schema ID (NSID + fragment if applicable)
    pub schema_id: String,
    /// The lexicon document
    pub doc: LexiconDoc<'static>,
    /// Runtime validation checks
    pub validation_checks: Vec<ValidationCheck>,
    /// Unresolved type refs (for two-pass resolution in workspace discovery)
    pub unresolved_refs: Vec<UnresolvedRef>,
    /// Union field mappings (schema_name -> type_path for runtime LEXICON_UNION_REFS access)
    pub union_fields: std::collections::BTreeMap<String, String>,
}

/// A reference to a type that couldn't be resolved at build time
#[derive(Debug, Clone)]
pub struct UnresolvedRef {
    /// The Rust type that needs resolution
    pub rust_type: String,
    /// Field path where this ref appears (e.g., "main.properties.author")
    pub field_path: String,
    /// Placeholder ref currently in the schema (will be replaced)
    pub placeholder_ref: String,
}

/// A runtime validation requirement
#[derive(Debug, Clone)]
pub struct ValidationCheck {
    /// Field name (Rust identifier)
    pub field_name: String,
    /// Schema field name (JSON name after serde rename)
    pub schema_name: String,
    /// Rust type path (for diagnostic purposes)
    pub field_type: String,
    /// Is this field required (not Option<T>)?
    pub is_required: bool,
    /// Is this validating an array length (vs string length)?
    pub is_array: bool,
    /// The specific constraint to check
    pub check: ConstraintCheck,
}

/// Specific constraint checks
#[derive(Debug, Clone)]
pub enum ConstraintCheck {
    MaxLength { max: usize },
    MaxGraphemes { max: usize },
    MinLength { min: usize },
    MinGraphemes { min: usize },
    Maximum { max: i64 },
    Minimum { min: i64 },
}

/// Parsed lexicon attributes from type
#[derive(Debug, Default)]
pub struct LexiconTypeAttrs {
    /// NSID for this type (required for primary types)
    pub nsid: Option<String>,
    /// Fragment name (None = not a fragment, Some("") = infer from type name)
    pub fragment: Option<String>,
    /// Type kind
    pub kind: Option<LexiconTypeKind>,
    /// Record key type (for records)
    pub key: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum LexiconTypeKind {
    Record,
    Query,
    Procedure,
    Subscription,
    Object,
    Union,
}

/// Parsed lexicon attributes from field
#[derive(Debug, Default, Clone)]
pub struct LexiconFieldAttrs {
    // Field-level constraints (for strings, integers, etc.)
    pub max_length: Option<usize>,
    pub max_graphemes: Option<usize>,
    pub min_length: Option<usize>,
    pub min_graphemes: Option<usize>,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,

    // Array-level constraints
    pub max_items: Option<usize>,
    pub min_items: Option<usize>,

    // Item-level constraints (for array items)
    pub item_max_length: Option<usize>,
    pub item_max_graphemes: Option<usize>,
    pub item_min_length: Option<usize>,
    pub item_min_graphemes: Option<usize>,

    pub explicit_ref: Option<String>,
    pub format: Option<String>,
    pub union_type: Option<Option<String>>, // None = no union, Some(None) = infer, Some(Some(name)) = explicit
}

/// Parsed serde attributes relevant to lexicon schema
#[derive(Debug, Default)]
pub struct SerdeAttrs {
    pub rename: Option<String>,
    pub skip: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum RenameRule {
    CamelCase,
    SnakeCase,
    PascalCase,
    ScreamingSnakeCase,
    KebabCase,
}

impl RenameRule {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "camelCase" => Some(RenameRule::CamelCase),
            "snake_case" => Some(RenameRule::SnakeCase),
            "PascalCase" => Some(RenameRule::PascalCase),
            "SCREAMING_SNAKE_CASE" => Some(RenameRule::ScreamingSnakeCase),
            "kebab-case" => Some(RenameRule::KebabCase),
            _ => None,
        }
    }

    pub fn apply(&self, input: &str) -> String {
        use heck::*;
        match self {
            RenameRule::CamelCase => input.to_lower_camel_case(),
            RenameRule::SnakeCase => input.to_snake_case(),
            RenameRule::PascalCase => input.to_pascal_case(),
            RenameRule::ScreamingSnakeCase => input.to_shouty_snake_case(),
            RenameRule::KebabCase => input.to_kebab_case(),
        }
    }
}

/// Field property (intermediate representation)
pub struct FieldProperty {
    /// Rust field name
    pub field_name: String,
    /// JSON field name (after serde rename)
    pub schema_name: String,
    /// Rust type
    pub rust_type: Type,
    /// Lexicon property
    pub property: LexObjectProperty<'static>,
    /// Is required?
    pub required: bool,
    /// Validation checks
    pub validations: Vec<ValidationCheck>,
    /// Unresolved refs from this field
    pub unresolved_refs: Vec<UnresolvedRef>,
    /// If this is a union field, contains the type path for accessing LEXICON_UNION_REFS at runtime
    pub union_type_path: Option<String>,
}
