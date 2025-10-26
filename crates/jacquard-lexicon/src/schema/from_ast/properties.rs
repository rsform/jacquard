//! Property building functions

use super::parse::{parse_field_attrs, parse_serde_attrs};
use super::types::*;
use crate::lexicon::*;
use crate::schema::type_mapping::{rust_type_to_lexicon_type, LexiconPrimitiveType};
use heck::ToLowerCamelCase;
use std::collections::BTreeMap;
use syn::Type;

/// Build object properties from struct fields
pub fn build_object_properties(
    fields: &syn::Fields,
    rename_rule: Option<RenameRule>,
) -> syn::Result<Vec<FieldProperty>> {
    let named_fields = match fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => {
            return Err(syn::Error::new_spanned(
                fields,
                "LexiconSchema only supports structs with named fields",
            ));
        }
    };

    let mut properties = Vec::new();

    for field in named_fields {
        let field_name = field.ident.as_ref().unwrap().to_string();

        // Skip extra_data field (added by #[lexicon] attribute macro)
        if field_name == "extra_data" {
            continue;
        }

        // Parse attributes
        let serde_attrs = parse_serde_attrs(&field.attrs)?;
        let lex_attrs = parse_field_attrs(&field.attrs)?;

        // Skip if serde(skip)
        if serde_attrs.skip {
            continue;
        }

        // Determine schema name
        let schema_name = if let Some(rename) = serde_attrs.rename {
            rename
        } else if let Some(rule) = rename_rule {
            rule.apply(&field_name)
        } else {
            field_name.clone()
        };

        // Determine if required (Option<T> = optional)
        let (inner_type, required) = super::parse::extract_option_inner(&field.ty);

        // Build property and validations
        let field_prop =
            build_field_property(&field_name, &schema_name, inner_type, required, &lex_attrs)?;

        properties.push(field_prop);
    }

    Ok(properties)
}

/// Build a single field property
fn build_field_property(
    field_name: &str,
    schema_name: &str,
    rust_type: &Type,
    required: bool,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<FieldProperty> {
    // Build the lexicon property
    let (property, mut unresolved_refs) = build_lex_property(rust_type, constraints)?;

    // Update field paths in unresolved refs
    for uref in &mut unresolved_refs {
        uref.field_path = format!("main.properties.{}", schema_name);
    }

    // Build validation checks
    let validations = build_validations(field_name, schema_name, rust_type, required, constraints)?;

    Ok(FieldProperty {
        field_name: field_name.to_string(),
        schema_name: schema_name.to_string(),
        rust_type: rust_type.clone(),
        property,
        required,
        validations,
        unresolved_refs,
    })
}

/// Build LexObjectProperty from Rust type and constraints
/// Returns (property, unresolved_refs)
fn build_lex_property(
    rust_type: &Type,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<(LexObjectProperty<'static>, Vec<UnresolvedRef>)> {
    // Try to detect primitive type
    let lex_type = rust_type_to_lexicon_type(rust_type);

    match lex_type {
        Some(LexiconPrimitiveType::Boolean) => Ok((LexObjectProperty::Boolean(LexBoolean {
            description: None,
            default: None,
            r#const: None,
        }), Vec::new())),
        Some(LexiconPrimitiveType::Integer) => Ok((LexObjectProperty::Integer(LexInteger {
            description: None,
            default: None,
            minimum: constraints.minimum,
            maximum: constraints.maximum,
            r#enum: None,
            r#const: None,
        }), Vec::new())),
        Some(LexiconPrimitiveType::String(format)) => Ok((LexObjectProperty::String(
            build_string_property(format, constraints),
        ), Vec::new())),
        Some(LexiconPrimitiveType::Bytes) => Ok((LexObjectProperty::Bytes(LexBytes {
            description: None,
            max_length: constraints.max_length,
            min_length: constraints.min_length,
        }), Vec::new())),
        Some(LexiconPrimitiveType::CidLink) => {
            Ok((LexObjectProperty::CidLink(LexCidLink { description: None }), Vec::new()))
        }
        Some(LexiconPrimitiveType::Blob) => Ok((LexObjectProperty::Blob(LexBlob {
            description: None,
            accept: None,
            max_size: None,
        }), Vec::new())),
        Some(LexiconPrimitiveType::Unknown) => {
            Ok((LexObjectProperty::Unknown(LexUnknown { description: None }), Vec::new()))
        }
        Some(LexiconPrimitiveType::Array(item_type)) => {
            let (item_prop, unresolved) = build_array_item(*item_type)?;
            Ok((LexObjectProperty::Array(LexArray {
                description: None,
                items: item_prop,
                min_length: constraints.min_length,
                max_length: constraints.max_length,
            }), unresolved))
        }
        Some(LexiconPrimitiveType::Object) => {
            // Nested object - shouldn't typically happen, use Unknown
            Ok((LexObjectProperty::Unknown(LexUnknown { description: None }), Vec::new()))
        }
        Some(LexiconPrimitiveType::Ref(ref_nsid)) => Ok((LexObjectProperty::Ref(LexRef {
            description: None,
            r#ref: ref_nsid.into(),
        }), Vec::new())),
        Some(LexiconPrimitiveType::Union(_refs)) => {
            // Union types detected - would need to be generated differently
            // For now, use Unknown
            Ok((LexObjectProperty::Unknown(LexUnknown { description: None }), Vec::new()))
        }
        None => {
            // Not a primitive - check for explicit ref
            if let Some(ref_nsid) = &constraints.explicit_ref {
                Ok((LexObjectProperty::Ref(LexRef {
                    description: None,
                    r#ref: ref_nsid.clone().into(),
                }), Vec::new()))
            } else {
                // Type doesn't have explicit ref - create placeholder and track as unresolved
                let type_str = quote::quote!(#rust_type).to_string();
                let placeholder = format!("#unresolved:{}", extract_type_name(&type_str));

                let unresolved = UnresolvedRef {
                    rust_type: type_str,
                    field_path: String::new(), // Will be filled in by caller
                    placeholder_ref: placeholder.clone(),
                };

                Ok((LexObjectProperty::Ref(LexRef {
                    description: None,
                    r#ref: placeholder.into(),
                }), vec![unresolved]))
            }
        }
    }
}

/// Extract simple type name from type path (e.g., "FeedViewPost" from "app::bsky::FeedViewPost")
fn extract_type_name(type_str: &str) -> String {
    type_str
        .split("::")
        .last()
        .unwrap_or(type_str)
        .trim_matches(|c| c == '<' || c == '>' || c == ' ')
        .to_string()
        .to_lower_camel_case()
}

/// Build array item property
/// Returns (item, unresolved_refs)
fn build_array_item(item_type: LexiconPrimitiveType) -> syn::Result<(LexArrayItem<'static>, Vec<UnresolvedRef>)> {
    match item_type {
        LexiconPrimitiveType::String(format) => {
            let format_enum = match format {
                crate::schema::type_mapping::StringFormat::Plain => None,
                crate::schema::type_mapping::StringFormat::Did => Some(LexStringFormat::Did),
                crate::schema::type_mapping::StringFormat::Handle => Some(LexStringFormat::Handle),
                crate::schema::type_mapping::StringFormat::AtUri => Some(LexStringFormat::AtUri),
                crate::schema::type_mapping::StringFormat::Nsid => Some(LexStringFormat::Nsid),
                crate::schema::type_mapping::StringFormat::Cid => Some(LexStringFormat::Cid),
                crate::schema::type_mapping::StringFormat::Datetime => {
                    Some(LexStringFormat::Datetime)
                }
                crate::schema::type_mapping::StringFormat::Language => {
                    Some(LexStringFormat::Language)
                }
                crate::schema::type_mapping::StringFormat::Tid => Some(LexStringFormat::Tid),
                crate::schema::type_mapping::StringFormat::RecordKey => {
                    Some(LexStringFormat::RecordKey)
                }
                crate::schema::type_mapping::StringFormat::AtIdentifier => {
                    Some(LexStringFormat::AtIdentifier)
                }
                crate::schema::type_mapping::StringFormat::Uri => Some(LexStringFormat::Uri),
            };
            Ok((LexArrayItem::String(LexString {
                description: None,
                format: format_enum,
                default: None,
                min_length: None,
                max_length: None,
                min_graphemes: None,
                max_graphemes: None,
                r#enum: None,
                r#const: None,
                known_values: None,
            }), Vec::new()))
        }
        LexiconPrimitiveType::Integer => Ok((LexArrayItem::Integer(LexInteger {
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            r#enum: None,
            r#const: None,
        }), Vec::new())),
        LexiconPrimitiveType::Boolean => Ok((LexArrayItem::Boolean(LexBoolean {
            description: None,
            default: None,
            r#const: None,
        }), Vec::new())),
        LexiconPrimitiveType::Bytes => Ok((LexArrayItem::Bytes(LexBytes {
            description: None,
            max_length: None,
            min_length: None,
        }), Vec::new())),
        LexiconPrimitiveType::CidLink => {
            Ok((LexArrayItem::CidLink(LexCidLink { description: None }), Vec::new()))
        }
        LexiconPrimitiveType::Blob => Ok((LexArrayItem::Blob(LexBlob {
            description: None,
            accept: None,
            max_size: None,
        }), Vec::new())),
        LexiconPrimitiveType::Unknown => {
            Ok((LexArrayItem::Unknown(LexUnknown { description: None }), Vec::new()))
        }
        LexiconPrimitiveType::Ref(ref_nsid) => Ok((LexArrayItem::Ref(LexRef {
            description: None,
            r#ref: ref_nsid.into(),
        }), Vec::new())),
        LexiconPrimitiveType::Object => {
            // Object in array - return empty object
            Ok((LexArrayItem::Object(LexObject {
                description: None,
                required: None,
                nullable: None,
                properties: BTreeMap::new(),
            }), Vec::new()))
        }
        LexiconPrimitiveType::Union(refs) => {
            // Union in array - create union with refs
            Ok((LexArrayItem::Union(LexRefUnion {
                description: None,
                refs: refs.into_iter().map(Into::into).collect(),
                closed: None,
            }), Vec::new()))
        }
        LexiconPrimitiveType::Array(_) => {
            // Nested arrays not supported in lexicon - return Unknown
            Ok((LexArrayItem::Unknown(LexUnknown {
                description: None,
            }), Vec::new()))
        }
    }
}

/// Build string property with format
fn build_string_property(
    format: crate::schema::type_mapping::StringFormat,
    constraints: &LexiconFieldAttrs,
) -> LexString<'static> {
    use crate::schema::type_mapping::StringFormat;

    let format_enum = match format {
        StringFormat::Plain => None,
        StringFormat::Did => Some(LexStringFormat::Did),
        StringFormat::Handle => Some(LexStringFormat::Handle),
        StringFormat::AtUri => Some(LexStringFormat::AtUri),
        StringFormat::Nsid => Some(LexStringFormat::Nsid),
        StringFormat::Cid => Some(LexStringFormat::Cid),
        StringFormat::Datetime => Some(LexStringFormat::Datetime),
        StringFormat::Language => Some(LexStringFormat::Language),
        StringFormat::Tid => Some(LexStringFormat::Tid),
        StringFormat::RecordKey => Some(LexStringFormat::RecordKey),
        StringFormat::AtIdentifier => Some(LexStringFormat::AtIdentifier),
        StringFormat::Uri => Some(LexStringFormat::Uri),
    };

    LexString {
        description: None,
        format: format_enum,
        default: None,
        min_length: constraints.min_length,
        max_length: constraints.max_length,
        min_graphemes: constraints.min_graphemes,
        max_graphemes: constraints.max_graphemes,
        r#enum: None,
        r#const: None,
        known_values: None,
    }
}

/// Build validation checks for a field
fn build_validations(
    field_name: &str,
    schema_name: &str,
    field_type: &Type,
    is_required: bool,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<Vec<ValidationCheck>> {
    let mut checks = Vec::new();
    let lex_type = rust_type_to_lexicon_type(field_type);

    let field_type_str = quote::quote!(#field_type).to_string();

    match lex_type {
        Some(LexiconPrimitiveType::String(_)) => {
            if let Some(max) = constraints.max_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    check: ConstraintCheck::MaxLength { max },
                });
            }
            if let Some(max) = constraints.max_graphemes {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    check: ConstraintCheck::MaxGraphemes { max },
                });
            }
            if let Some(min) = constraints.min_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    check: ConstraintCheck::MinLength { min },
                });
            }
            if let Some(min) = constraints.min_graphemes {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str,
                    is_required,
                    check: ConstraintCheck::MinGraphemes { min },
                });
            }
        }
        Some(LexiconPrimitiveType::Integer) => {
            if let Some(max) = constraints.maximum {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    check: ConstraintCheck::Maximum { max },
                });
            }
            if let Some(min) = constraints.minimum {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str,
                    is_required,
                    check: ConstraintCheck::Minimum { min },
                });
            }
        }
        Some(LexiconPrimitiveType::Array(_)) => {
            if let Some(max) = constraints.max_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    check: ConstraintCheck::MaxLength { max },
                });
            }
            if let Some(min) = constraints.min_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str,
                    is_required,
                    check: ConstraintCheck::MinLength { min },
                });
            }
        }
        _ => {
            // No validation for other types
        }
    }

    Ok(checks)
}
