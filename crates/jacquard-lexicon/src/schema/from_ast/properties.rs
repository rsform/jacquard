//! Property building functions

use super::parse::{parse_field_attrs, parse_serde_attrs};
use super::types::*;
use crate::lexicon::*;
use crate::schema::type_mapping::{LexiconPrimitiveType, rust_type_to_lexicon_type};
use heck::ToLowerCamelCase;
use std::collections::BTreeMap;
use syn::Type;
use syn::ext::IdentExt;

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
        // Strip r# prefix from raw identifiers (r#type -> type)
        let field_name = field.ident.as_ref().unwrap().unraw().to_string();

        // Skip extra_data field (added by #[lexicon] attribute macro)
        if field_name == "extra_data" {
            continue;
        }

        // Parse attributes
        let serde_attrs = parse_serde_attrs(&field.attrs)?;
        let lex_attrs = parse_field_attrs(&field.attrs)?;
        let doc_comment = extract_doc_comment(&field.attrs);

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
        let field_prop = build_field_property(
            &field_name,
            &schema_name,
            inner_type,
            required,
            &lex_attrs,
            doc_comment,
        )?;

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
    description: Option<String>,
) -> syn::Result<FieldProperty> {
    // Build the lexicon property
    let (mut property, mut unresolved_refs, union_type_path) =
        build_lex_property(rust_type, constraints)?;

    // Add description if present
    if let Some(desc) = description {
        property = add_description_to_property(property, desc);
    }

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
        union_type_path,
    })
}

/// Build LexObjectProperty from Rust type and constraints
/// Returns (property, unresolved_refs, union_type_path)
fn build_lex_property(
    rust_type: &Type,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<(
    LexObjectProperty<'static>,
    Vec<UnresolvedRef>,
    Option<String>,
)> {
    // Check if this is a union field
    if constraints.union_type.is_some() {
        // Store the actual syn::Type for later code generation
        // We'll serialize it to string for now, but we need the TokenStream
        let type_tokens = quote::quote!(#rust_type);

        // Create a placeholder union property (refs will be filled at runtime)
        let placeholder_union = LexObjectProperty::Union(LexRefUnion {
            description: None,
            refs: vec![], // Will be filled at runtime by accessing Type::LEXICON_UNION_REFS
            closed: None,
        });

        return Ok((placeholder_union, Vec::new(), Some(type_tokens.to_string())));
    }

    // Check for explicit ref first (overrides type detection)
    if let Some(ref_nsid) = &constraints.explicit_ref {
        // Check if it's an array with explicit ref
        if let Some(_inner) = extract_vec_inner(rust_type) {
            return Ok((
                LexObjectProperty::Array(LexArray {
                    description: None,
                    items: LexArrayItem::Ref(LexRef {
                        description: None,
                        r#ref: ref_nsid.clone().into(),
                    }),
                    min_length: constraints.min_items,
                    max_length: constraints.max_items,
                }),
                Vec::new(),
                None,
            ));
        } else {
            // Non-array field with explicit ref
            return Ok((
                LexObjectProperty::Ref(LexRef {
                    description: None,
                    r#ref: ref_nsid.clone().into(),
                }),
                Vec::new(),
                None,
            ));
        }
    }

    // Try to detect primitive type
    let lex_type = rust_type_to_lexicon_type(rust_type);

    match lex_type {
        Some(LexiconPrimitiveType::Boolean) => Ok((
            LexObjectProperty::Boolean(LexBoolean {
                description: None,
                default: None,
                r#const: None,
            }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::Integer) => Ok((
            LexObjectProperty::Integer(LexInteger {
                description: None,
                default: None,
                minimum: constraints.minimum,
                maximum: constraints.maximum,
                r#enum: None,
                r#const: None,
            }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::String(format)) => Ok((
            LexObjectProperty::String(build_string_property(format, constraints)),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::Bytes) => Ok((
            LexObjectProperty::Bytes(LexBytes {
                description: None,
                max_length: constraints.max_length,
                min_length: constraints.min_length,
            }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::CidLink) => Ok((
            LexObjectProperty::CidLink(LexCidLink { description: None }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::Blob) => Ok((
            LexObjectProperty::Blob(LexBlob {
                description: None,
                accept: None,
                max_size: None,
            }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::Unknown) => Ok((
            LexObjectProperty::Unknown(LexUnknown { description: None }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::Array(item_type)) => {
            let (item_prop, unresolved) = build_array_item(*item_type, constraints)?;
            Ok((
                LexObjectProperty::Array(LexArray {
                    description: None,
                    items: item_prop,
                    min_length: constraints.min_items,
                    max_length: constraints.max_items,
                }),
                unresolved,
                None,
            ))
        }
        Some(LexiconPrimitiveType::Object) => {
            // Nested object - shouldn't typically happen, use Unknown
            Ok((
                LexObjectProperty::Unknown(LexUnknown { description: None }),
                Vec::new(),
                None,
            ))
        }
        Some(LexiconPrimitiveType::Ref(ref_nsid)) => Ok((
            LexObjectProperty::Ref(LexRef {
                description: None,
                r#ref: ref_nsid.into(),
            }),
            Vec::new(),
            None,
        )),
        Some(LexiconPrimitiveType::Union(_refs)) => {
            // Union types detected - would need to be generated differently
            // For now, use Unknown
            Ok((
                LexObjectProperty::Unknown(LexUnknown { description: None }),
                Vec::new(),
                None,
            ))
        }
        None => {
            // Not a primitive - check if it's Vec<CustomType> first
            if let Some(inner_type) = extract_vec_inner(rust_type) {
                // It's a Vec - build array with ref item
                let fragment_name = extract_type_ident(inner_type).to_lower_camel_case();
                let local_ref = format!("#{}", fragment_name);

                Ok((
                    LexObjectProperty::Array(LexArray {
                        description: None,
                        items: LexArrayItem::Ref(LexRef {
                            description: None,
                            r#ref: local_ref.into(),
                        }),
                        min_length: constraints.min_items,
                        max_length: constraints.max_items,
                    }),
                    Vec::new(),
                    None,
                ))
            } else {
                // Not a Vec - assume local fragment
                let fragment_name = extract_type_ident(rust_type).to_lower_camel_case();
                let local_ref = format!("#{}", fragment_name);

                Ok((
                    LexObjectProperty::Ref(LexRef {
                        description: None,
                        r#ref: local_ref.into(),
                    }),
                    Vec::new(),
                    None,
                ))
            }
        }
    }
}

/// Extract the inner type from Vec<T>, returns Some(T) if this is a Vec, None otherwise
fn extract_vec_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let last_segment = type_path.path.segments.last()?;
        if last_segment.ident == "Vec" {
            if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                // Find first Type argument (skip lifetimes)
                for arg in &args.args {
                    if let syn::GenericArgument::Type(inner_ty) = arg {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

/// Extract the type identifier from a syn::Type, ignoring lifetimes and other generic params
/// E.g., Entity<'a> -> "Entity", Vec<String> -> "Vec"
fn extract_type_ident(ty: &Type) -> String {
    if let Type::Path(type_path) = ty {
        if let Some(last_segment) = type_path.path.segments.last() {
            return last_segment.ident.to_string();
        }
    }
    // Fallback: shouldn't happen but return something
    "Unknown".to_string()
}

/// Build array item property
/// Returns (item, unresolved_refs)
fn build_array_item(
    item_type: LexiconPrimitiveType,
    constraints: &LexiconFieldAttrs,
) -> syn::Result<(LexArrayItem<'static>, Vec<UnresolvedRef>)> {
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
            Ok((
                LexArrayItem::String(LexString {
                    description: None,
                    format: format_enum,
                    default: None,
                    min_length: constraints.item_min_length,
                    max_length: constraints.item_max_length,
                    min_graphemes: constraints.item_min_graphemes,
                    max_graphemes: constraints.item_max_graphemes,
                    r#enum: None,
                    r#const: None,
                    known_values: None,
                }),
                Vec::new(),
            ))
        }
        LexiconPrimitiveType::Integer => Ok((
            LexArrayItem::Integer(LexInteger {
                description: None,
                default: None,
                minimum: None,
                maximum: None,
                r#enum: None,
                r#const: None,
            }),
            Vec::new(),
        )),
        LexiconPrimitiveType::Boolean => Ok((
            LexArrayItem::Boolean(LexBoolean {
                description: None,
                default: None,
                r#const: None,
            }),
            Vec::new(),
        )),
        LexiconPrimitiveType::Bytes => Ok((
            LexArrayItem::Bytes(LexBytes {
                description: None,
                max_length: None,
                min_length: None,
            }),
            Vec::new(),
        )),
        LexiconPrimitiveType::CidLink => Ok((
            LexArrayItem::CidLink(LexCidLink { description: None }),
            Vec::new(),
        )),
        LexiconPrimitiveType::Blob => Ok((
            LexArrayItem::Blob(LexBlob {
                description: None,
                accept: None,
                max_size: None,
            }),
            Vec::new(),
        )),
        LexiconPrimitiveType::Unknown => Ok((
            LexArrayItem::Unknown(LexUnknown { description: None }),
            Vec::new(),
        )),
        LexiconPrimitiveType::Ref(ref_nsid) => Ok((
            LexArrayItem::Ref(LexRef {
                description: None,
                r#ref: ref_nsid.into(),
            }),
            Vec::new(),
        )),
        LexiconPrimitiveType::Object => {
            // Object in array - return empty object
            Ok((
                LexArrayItem::Object(LexObject {
                    description: None,
                    required: None,
                    nullable: None,
                    properties: BTreeMap::new(),
                }),
                Vec::new(),
            ))
        }
        LexiconPrimitiveType::Union(refs) => {
            // Union in array - create union with refs
            Ok((
                LexArrayItem::Union(LexRefUnion {
                    description: None,
                    refs: refs.into_iter().map(Into::into).collect(),
                    closed: None,
                }),
                Vec::new(),
            ))
        }
        LexiconPrimitiveType::Array(_) => {
            // Nested arrays not supported in lexicon - return Unknown
            Ok((
                LexArrayItem::Unknown(LexUnknown { description: None }),
                Vec::new(),
            ))
        }
    }
}

/// Build string property with format
fn build_string_property(
    format: crate::schema::type_mapping::StringFormat,
    constraints: &LexiconFieldAttrs,
) -> LexString<'static> {
    use crate::schema::type_mapping::StringFormat;

    // Check if format is overridden by attribute
    let effective_format = if let Some(format_str) = &constraints.format {
        // Parse format string to StringFormat
        match format_str.as_str() {
            "did" => StringFormat::Did,
            "handle" => StringFormat::Handle,
            "at-uri" => StringFormat::AtUri,
            "nsid" => StringFormat::Nsid,
            "cid" => StringFormat::Cid,
            "datetime" => StringFormat::Datetime,
            "language" => StringFormat::Language,
            "tid" => StringFormat::Tid,
            "record-key" => StringFormat::RecordKey,
            "at-identifier" => StringFormat::AtIdentifier,
            "uri" => StringFormat::Uri,
            _ => format, // Unknown format, use type-detected format
        }
    } else {
        format
    };

    let format_enum = match effective_format {
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
                    is_array: false,
                    check: ConstraintCheck::MaxLength { max },
                });
            }
            if let Some(max) = constraints.max_graphemes {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    is_array: false,
                    check: ConstraintCheck::MaxGraphemes { max },
                });
            }
            if let Some(min) = constraints.min_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    is_array: false,
                    check: ConstraintCheck::MinLength { min },
                });
            }
            if let Some(min) = constraints.min_graphemes {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str,
                    is_required,
                    is_array: false,
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
                    is_array: false,
                    check: ConstraintCheck::Maximum { max },
                });
            }
            if let Some(min) = constraints.minimum {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str,
                    is_required,
                    is_array: false,
                    check: ConstraintCheck::Minimum { min },
                });
            }
        }
        Some(LexiconPrimitiveType::Array(_)) => {
            if let Some(max) = constraints.max_items {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str.clone(),
                    is_required,
                    is_array: true,
                    check: ConstraintCheck::MaxLength { max },
                });
            }
            if let Some(min) = constraints.min_items {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: field_type_str,
                    is_required,
                    is_array: true,
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

/// Extract doc comment from attributes
pub(super) fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let mut docs = Vec::new();

    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                }) = &nv.value
                {
                    let doc_line = lit_str.value();
                    // Strip leading space that rustdoc adds
                    let trimmed = doc_line.strip_prefix(' ').unwrap_or(&doc_line);
                    docs.push(trimmed.to_string());
                }
            }
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

/// Add description to a property
fn add_description_to_property(
    property: LexObjectProperty<'static>,
    description: String,
) -> LexObjectProperty<'static> {
    use crate::lexicon::*;
    use jacquard_common::CowStr;

    let desc = Some(CowStr::copy_from_str(&description));

    match property {
        LexObjectProperty::String(mut s) => {
            s.description = desc;
            LexObjectProperty::String(s)
        }
        LexObjectProperty::Integer(mut i) => {
            i.description = desc;
            LexObjectProperty::Integer(i)
        }
        LexObjectProperty::Boolean(mut b) => {
            b.description = desc;
            LexObjectProperty::Boolean(b)
        }
        LexObjectProperty::Bytes(mut b) => {
            b.description = desc;
            LexObjectProperty::Bytes(b)
        }
        LexObjectProperty::CidLink(mut c) => {
            c.description = desc;
            LexObjectProperty::CidLink(c)
        }
        LexObjectProperty::Blob(mut b) => {
            b.description = desc;
            LexObjectProperty::Blob(b)
        }
        LexObjectProperty::Ref(mut r) => {
            r.description = desc;
            LexObjectProperty::Ref(r)
        }
        LexObjectProperty::Unknown(mut u) => {
            u.description = desc;
            LexObjectProperty::Unknown(u)
        }
        LexObjectProperty::Array(mut a) => {
            a.description = desc;
            LexObjectProperty::Array(a)
        }
        LexObjectProperty::Union(mut u) => {
            u.description = desc;
            LexObjectProperty::Union(u)
        }
        other => other,
    }
}
