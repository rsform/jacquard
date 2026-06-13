//! Type mapping utilities for converting Rust types to lexicon primitives
//!
//! These utilities parse Rust types using `syn` to determine their lexicon equivalents.
//! Used by the derive macro in Phase 2.

use syn;

/// Type-mapping context from the item that owns the field being mapped.
#[derive(Debug, Clone, Default)]
pub struct TypeMappingContext {
    string_type_params: Vec<syn::Ident>,
}

impl TypeMappingContext {
    /// Build mapping context from a type's generics.
    pub fn from_generics(generics: &syn::Generics) -> Self {
        let string_type_params = generics
            .type_params()
            .filter(|param| type_param_is_string_backing(param))
            .map(|param| param.ident.clone())
            .collect();

        Self { string_type_params }
    }

    fn is_string_type_param(&self, ident: &syn::Ident) -> bool {
        self.string_type_params.iter().any(|param| param == ident)
    }
}

fn type_param_is_string_backing(param: &syn::TypeParam) -> bool {
    param.bounds.iter().any(type_param_bound_is_string_backing)
}

fn type_param_bound_is_string_backing(bound: &syn::TypeParamBound) -> bool {
    let syn::TypeParamBound::Trait(bound) = bound else {
        return false;
    };

    let Some(last_segment) = bound.path.segments.last() else {
        return false;
    };

    if last_segment.ident == "BosStr" {
        true
    } else if last_segment.ident == "Bos" {
        matches!(
            &last_segment.arguments,
            syn::PathArguments::AngleBracketed(args)
                if args.args.iter().any(|arg| matches!(arg, syn::GenericArgument::Type(syn::Type::Path(path)) if path.path.is_ident("str")))
        )
    } else {
        false
    }
}

/// Detect the lexicon type for a Rust type path.
///
/// Used by derive macro to map field types to lexicon primitives.
pub fn rust_type_to_lexicon_type(ty: &syn::Type) -> Option<LexiconPrimitiveType> {
    rust_type_to_lexicon_type_with_context(ty, &TypeMappingContext::default())
}

/// Detect the lexicon type for a Rust type path with enclosing item context.
pub fn rust_type_to_lexicon_type_with_context(
    ty: &syn::Type,
    context: &TypeMappingContext,
) -> Option<LexiconPrimitiveType> {
    match ty {
        syn::Type::Path(type_path) => {
            let path = &type_path.path;
            let last_segment = path.segments.last()?;

            if path.segments.len() == 1 && context.is_string_type_param(&last_segment.ident) {
                return Some(LexiconPrimitiveType::String(StringFormat::Plain));
            }

            let ident = &last_segment.ident;

            // Boolean types.
            if ident == "bool" {
                Some(LexiconPrimitiveType::Boolean)
            // Integer types. Lexicon integers are i64; unsigned Rust integer
            // fields are still represented by the lexicon integer primitive.
            } else if ident == "i8"
                || ident == "i16"
                || ident == "i32"
                || ident == "i64"
                || ident == "isize"
                || ident == "u8"
                || ident == "u16"
                || ident == "u32"
                || ident == "u64"
                || ident == "usize"
            {
                Some(LexiconPrimitiveType::Integer)
            // String types.
            } else if ident == "String" || ident == "str" || ident == "CowStr" || ident == "SmolStr"
            {
                Some(LexiconPrimitiveType::String(StringFormat::Plain))
            } else if ident == "Did" {
                Some(LexiconPrimitiveType::String(StringFormat::Did))
            } else if ident == "Handle" {
                Some(LexiconPrimitiveType::String(StringFormat::Handle))
            } else if ident == "AtUri" {
                Some(LexiconPrimitiveType::String(StringFormat::AtUri))
            } else if ident == "Nsid" {
                Some(LexiconPrimitiveType::String(StringFormat::Nsid))
            } else if ident == "Cid" {
                Some(LexiconPrimitiveType::String(StringFormat::Cid))
            } else if ident == "Datetime" {
                Some(LexiconPrimitiveType::String(StringFormat::Datetime))
            } else if ident == "Language" {
                Some(LexiconPrimitiveType::String(StringFormat::Language))
            } else if ident == "Tid" {
                Some(LexiconPrimitiveType::String(StringFormat::Tid))
            } else if ident == "RecordKey" {
                Some(LexiconPrimitiveType::String(StringFormat::RecordKey))
            // IPLD types.
            } else if ident == "Bytes" && is_bytes_type(path) {
                Some(LexiconPrimitiveType::Bytes)
            } else if ident == "CidLink" {
                Some(LexiconPrimitiveType::CidLink)
            // Blob types.
            } else if ident == "Blob" || ident == "BlobRef" {
                Some(LexiconPrimitiveType::Blob)
            // Unknown/unvalidated data.
            } else if ident == "Data" || ident == "RawData" {
                Some(LexiconPrimitiveType::Unknown)
            } else if ident == "Vec" {
                // Extract Vec<T> item type.
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(LexiconPrimitiveType::Array(Box::new(
                            rust_type_to_lexicon_type_with_context(inner_ty, context)?,
                        )));
                    }
                }
                None
            } else if ident == "Option" {
                // Extract Option<T> inner type.
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return rust_type_to_lexicon_type_with_context(inner_ty, context);
                    }
                }
                None
            } else {
                None
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

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn context_from_item(input: syn::DeriveInput) -> TypeMappingContext {
        TypeMappingContext::from_generics(&input.generics)
    }

    #[test]
    fn unconstrained_type_param_has_no_primitive_mapping() {
        let ty: syn::Type = parse_quote!(S);
        let context = TypeMappingContext::default();

        assert_eq!(rust_type_to_lexicon_type_with_context(&ty, &context), None);
    }

    #[test]
    fn bosstr_type_param_maps_to_string() {
        let input: syn::DeriveInput = parse_quote! {
            struct Generic<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
                text: S,
            }
        };
        let ty: syn::Type = parse_quote!(S);
        let context = context_from_item(input);

        assert_eq!(
            rust_type_to_lexicon_type_with_context(&ty, &context),
            Some(LexiconPrimitiveType::String(StringFormat::Plain))
        );
    }

    #[test]
    fn non_s_bosstr_type_param_maps_to_string() {
        let input: syn::DeriveInput = parse_quote! {
            struct Generic<Text: jacquard_common::BosStr = jacquard_common::DefaultStr> {
                text: Text,
            }
        };
        let ty: syn::Type = parse_quote!(Text);
        let context = context_from_item(input);

        assert_eq!(
            rust_type_to_lexicon_type_with_context(&ty, &context),
            Some(LexiconPrimitiveType::String(StringFormat::Plain))
        );
    }

    #[test]
    fn vec_bosstr_type_param_maps_to_string_array() {
        let input: syn::DeriveInput = parse_quote! {
            struct Generic<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
                tags: Vec<S>,
            }
        };
        let ty: syn::Type = parse_quote!(Vec<S>);
        let context = context_from_item(input);

        assert_eq!(
            rust_type_to_lexicon_type_with_context(&ty, &context),
            Some(LexiconPrimitiveType::Array(Box::new(
                LexiconPrimitiveType::String(StringFormat::Plain)
            )))
        );
    }

    #[test]
    fn option_bosstr_type_param_maps_to_inner_string() {
        let input: syn::DeriveInput = parse_quote! {
            struct Generic<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
                text: Option<S>,
            }
        };
        let ty: syn::Type = parse_quote!(Option<S>);
        let context = context_from_item(input);

        assert_eq!(
            rust_type_to_lexicon_type_with_context(&ty, &context),
            Some(LexiconPrimitiveType::String(StringFormat::Plain))
        );
    }

    #[test]
    fn clone_only_type_param_does_not_map_to_string() {
        let input: syn::DeriveInput = parse_quote! {
            struct Generic<S: Clone> {
                value: S,
            }
        };
        let ty: syn::Type = parse_quote!(S);
        let context = context_from_item(input);

        assert_eq!(rust_type_to_lexicon_type_with_context(&ty, &context), None);
    }

    #[test]
    fn old_style_bos_type_param_maps_to_string() {
        let input: syn::DeriveInput = parse_quote! {
            struct Generic<
                S: jacquard_common::Bos<str> + AsRef<str> = jacquard_common::DefaultStr,
            > {
                text: S,
            }
        };
        let ty: syn::Type = parse_quote!(S);
        let context = context_from_item(input);

        assert_eq!(
            rust_type_to_lexicon_type_with_context(&ty, &context),
            Some(LexiconPrimitiveType::String(StringFormat::Plain))
        );
    }
}
