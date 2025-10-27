//! Type mapping utilities for converting Rust types to lexicon primitives
//!
//! These utilities parse Rust types using `syn` to determine their lexicon equivalents.
//! Used by the derive macro in Phase 2.

use syn;

/// Detect the lexicon type for a Rust type path
///
/// Used by derive macro to map field types to lexicon primitives.
pub fn rust_type_to_lexicon_type(ty: &syn::Type) -> Option<LexiconPrimitiveType> {
    match ty {
        syn::Type::Path(type_path) => {
            let path = &type_path.path;
            let last_segment = path.segments.last()?;

            match last_segment.ident.to_string().as_str() {
                // Boolean types
                "bool" => Some(LexiconPrimitiveType::Boolean),

                // Integer types (lexicon integers are i64)
                "i8" | "i16" | "i32" | "i64" | "isize" => Some(LexiconPrimitiveType::Integer),
                // Note: unsigned types not directly supported by lexicon spec
                // Users should use i64 or cast to i64
                "u8" | "u16" | "u32" | "u64" | "usize" => Some(LexiconPrimitiveType::Integer),

                // String types (Rust primitives)
                "String" | "str" => Some(LexiconPrimitiveType::String(StringFormat::Plain)),

                // jacquard string types
                "CowStr" | "SmolStr" => Some(LexiconPrimitiveType::String(StringFormat::Plain)),
                "Did" => Some(LexiconPrimitiveType::String(StringFormat::Did)),
                "Handle" => Some(LexiconPrimitiveType::String(StringFormat::Handle)),
                "AtUri" => Some(LexiconPrimitiveType::String(StringFormat::AtUri)),
                "Nsid" => Some(LexiconPrimitiveType::String(StringFormat::Nsid)),
                "Cid" => Some(LexiconPrimitiveType::String(StringFormat::Cid)),
                "Datetime" => Some(LexiconPrimitiveType::String(StringFormat::Datetime)),
                "Language" => Some(LexiconPrimitiveType::String(StringFormat::Language)),
                "Tid" => Some(LexiconPrimitiveType::String(StringFormat::Tid)),
                "RecordKey" => Some(LexiconPrimitiveType::String(StringFormat::RecordKey)),

                // IPLD types
                "Bytes" if is_bytes_type(path) => Some(LexiconPrimitiveType::Bytes),
                "CidLink" => Some(LexiconPrimitiveType::CidLink),

                // Blob type
                "Blob" | "BlobRef" => Some(LexiconPrimitiveType::Blob),

                // Unknown/unvalidated data
                "Data" | "RawData" => Some(LexiconPrimitiveType::Unknown),
                "Vec" => {
                    // Extract Vec<T> item type
                    if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            return Some(LexiconPrimitiveType::Array(Box::new(
                                rust_type_to_lexicon_type(inner_ty)?,
                            )));
                        }
                    }
                    None
                }
                "Option" => {
                    // Extract Option<T> inner type - mark as optional
                    if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            return rust_type_to_lexicon_type(inner_ty);
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Check if a path represents bytes::Bytes
fn is_bytes_type(path: &syn::Path) -> bool {
    if path.segments.len() == 2 {
        let first = &path.segments[0].ident;
        let second = &path.segments[1].ident;
        first == "bytes" && second == "Bytes"
    } else {
        false
    }
}

/// Classification of lexicon primitive types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexiconPrimitiveType {
    Boolean,
    Integer,
    String(StringFormat),
    Bytes,
    CidLink,
    Blob,
    Unknown,
    Array(Box<LexiconPrimitiveType>),
    Object,             // For structs
    Ref(String),        // For types with LexiconSchema impl
    Union(Vec<String>), // For enums with #[open_union]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringFormat {
    Plain,
    Did,
    Handle,
    AtUri,
    Nsid,
    Cid,
    Datetime,
    Language,
    Tid,
    RecordKey,
    AtIdentifier,
    Uri,
}

/// Extract constraints from field attributes
pub fn extract_field_constraints(attrs: &[syn::Attribute]) -> FieldConstraints {
    let mut constraints = FieldConstraints::default();

    for attr in attrs {
        if !attr.path().is_ident("lexicon") {
            continue;
        }

        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("max_length") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitInt>() {
                    constraints.max_length = Some(lit.base10_parse()?);
                }
            } else if meta.path.is_ident("max_graphemes") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitInt>() {
                    constraints.max_graphemes = Some(lit.base10_parse()?);
                }
            } else if meta.path.is_ident("min_length") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitInt>() {
                    constraints.min_length = Some(lit.base10_parse()?);
                }
            } else if meta.path.is_ident("min_graphemes") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitInt>() {
                    constraints.min_graphemes = Some(lit.base10_parse()?);
                }
            } else if meta.path.is_ident("minimum") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitInt>() {
                    constraints.minimum = Some(lit.base10_parse()?);
                }
            } else if meta.path.is_ident("maximum") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitInt>() {
                    constraints.maximum = Some(lit.base10_parse()?);
                }
            } else if meta.path.is_ident("ref") {
                if let Ok(lit) = meta.value()?.parse::<syn::LitStr>() {
                    constraints.explicit_ref = Some(lit.value());
                }
            }
            Ok(())
        });
    }

    constraints
}

#[derive(Debug, Default, Clone)]
pub struct FieldConstraints {
    pub max_length: Option<usize>,
    pub max_graphemes: Option<usize>,
    pub min_length: Option<usize>,
    pub min_graphemes: Option<usize>,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub explicit_ref: Option<String>,
}
