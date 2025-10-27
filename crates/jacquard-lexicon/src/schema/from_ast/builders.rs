//! Top-level builder functions

use super::parse::{
    determine_nsid, extract_variant_ref, has_open_union_attr, parse_serde_rename_all,
    parse_type_attrs,
};
use super::properties::build_object_properties;
use super::types::*;
use crate::lexicon::*;
use heck::ToLowerCamelCase;
use jacquard_common::smol_str::SmolStr;
use std::collections::BTreeMap;
use syn::DeriveInput;

/// Build schema from a struct
pub fn build_struct_schema(input: &DeriveInput) -> syn::Result<BuiltSchema> {
    // Parse type-level attributes
    let type_attrs = parse_type_attrs(&input.attrs)?;

    // Determine NSID
    let nsid = determine_nsid(&type_attrs, input)?;

    // Parse fields based on data type
    let data_struct = match &input.data {
        syn::Data::Struct(data_struct) => data_struct,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "build_struct_schema requires a struct",
            ));
        }
    };

    // Parse serde container attributes
    let rename_all = parse_serde_rename_all(&input.attrs)?;

    // Build properties
    let field_properties = build_object_properties(&data_struct.fields, rename_all)?;

    // Extract properties map, required list, and unresolved refs
    let mut properties = BTreeMap::new();
    let mut required = Vec::new();
    let mut all_validations = Vec::new();
    let mut all_unresolved = Vec::new();
    let mut union_fields = BTreeMap::new();

    for field_prop in field_properties {
        properties.insert(field_prop.schema_name.clone().into(), field_prop.property);
        if field_prop.required {
            required.push(field_prop.schema_name.clone().into());
        }
        if let Some(union_type_path) = field_prop.union_type_path {
            union_fields.insert(field_prop.schema_name.clone(), union_type_path);
        }
        all_validations.extend(field_prop.validations);
        all_unresolved.extend(field_prop.unresolved_refs);
    }

    // Extract doc comment for struct/record description
    let description = super::properties::extract_doc_comment(&input.attrs);

    // Build main def based on kind
    let user_type = build_user_type(&type_attrs, properties, required, description)?;

    // Determine def_name and schema_id (add fragment if needed)
    let (def_name, schema_id) = if let Some(fragment) = &type_attrs.fragment {
        let frag_name = if fragment.is_empty() {
            input.ident.to_string().to_lower_camel_case()
        } else {
            fragment.clone()
        };
        (frag_name.clone(), format!("{}#{}", nsid, frag_name))
    } else {
        ("main".to_string(), nsid.clone())
    };

    // Build lexicon doc with def under proper name
    let mut defs = BTreeMap::new();
    defs.insert(def_name.into(), user_type);

    let doc = LexiconDoc {
        lexicon: Lexicon::Lexicon1,
        id: nsid.clone().into(),
        revision: None,
        description: None,
        defs,
    };

    Ok(BuiltSchema {
        nsid,
        schema_id,
        doc,
        validation_checks: all_validations,
        unresolved_refs: all_unresolved,
        union_fields,
    })
}

/// Build LexUserType based on kind
fn build_user_type(
    type_attrs: &LexiconTypeAttrs,
    properties: BTreeMap<SmolStr, LexObjectProperty<'static>>,
    required: Vec<SmolStr>,
    description: Option<String>,
) -> syn::Result<LexUserType<'static>> {
    use jacquard_common::CowStr;

    let required_field = if required.is_empty() {
        None
    } else {
        Some(required)
    };

    let desc = description.as_ref().map(|s| CowStr::copy_from_str(s));

    match type_attrs.kind {
        Some(LexiconTypeKind::Record) => {
            // For records, description goes on the LexRecord, not the inner LexObject
            let obj = LexObject {
                description: None,
                required: required_field,
                nullable: None,
                properties,
            };
            Ok(LexUserType::Record(LexRecord {
                description: desc,
                key: type_attrs.key.clone().map(Into::into),
                record: LexRecordRecord::Object(obj),
            }))
        }
        Some(LexiconTypeKind::Query) => {
            // Convert properties to parameters
            let params = LexXrpcParameters {
                description: None,
                required: required_field,
                properties: properties
                    .into_iter()
                    .map(|(k, v)| (k, convert_object_prop_to_param_prop(v)))
                    .collect(),
            };
            Ok(LexUserType::XrpcQuery(LexXrpcQuery {
                description: None,
                parameters: Some(LexXrpcQueryParameter::Params(params)),
                output: None,
                errors: None,
            }))
        }
        Some(LexiconTypeKind::Procedure) => {
            let obj = LexObject {
                description: None,
                required: required_field,
                nullable: None,
                properties,
            };
            Ok(LexUserType::XrpcProcedure(LexXrpcProcedure {
                description: None,
                parameters: None,
                input: Some(LexXrpcBody {
                    description: None,
                    encoding: "application/json".into(),
                    schema: Some(LexXrpcBodySchema::Object(obj)),
                }),
                output: None,
                errors: None,
            }))
        }
        Some(LexiconTypeKind::Subscription) => {
            let params = LexXrpcParameters {
                description: None,
                required: required_field,
                properties: properties
                    .into_iter()
                    .map(|(k, v)| (k, convert_object_prop_to_param_prop(v)))
                    .collect(),
            };
            Ok(LexUserType::XrpcSubscription(LexXrpcSubscription {
                description: None,
                parameters: Some(LexXrpcSubscriptionParameter::Params(params)),
                message: None,
                infos: None,
                errors: None,
            }))
        }
        _ => {
            // For plain objects (fragments), description goes on the LexObject
            let obj = LexObject {
                description: desc,
                required: required_field,
                nullable: None,
                properties,
            };
            Ok(LexUserType::Object(obj))
        }
    }
}

/// Convert LexObjectProperty to LexXrpcParametersProperty
fn convert_object_prop_to_param_prop(
    prop: LexObjectProperty<'static>,
) -> LexXrpcParametersProperty<'static> {
    match prop {
        LexObjectProperty::Boolean(b) => LexXrpcParametersProperty::Boolean(b),
        LexObjectProperty::Integer(i) => LexXrpcParametersProperty::Integer(i),
        LexObjectProperty::String(s) => LexXrpcParametersProperty::String(s),
        LexObjectProperty::Unknown(u) => LexXrpcParametersProperty::Unknown(u),
        LexObjectProperty::Array(a) => {
            // Convert LexArray to LexPrimitiveArray
            let primitive_item = match a.items {
                LexArrayItem::Boolean(b) => LexPrimitiveArrayItem::Boolean(b),
                LexArrayItem::Integer(i) => LexPrimitiveArrayItem::Integer(i),
                LexArrayItem::String(s) => LexPrimitiveArrayItem::String(s),
                // Non-primitive items become Unknown
                _ => LexPrimitiveArrayItem::Unknown(LexUnknown { description: None }),
            };
            LexXrpcParametersProperty::Array(LexPrimitiveArray {
                description: a.description,
                items: primitive_item,
                min_length: a.min_length,
                max_length: a.max_length,
            })
        }
        // Other types not valid in parameters - shouldn't happen
        _ => LexXrpcParametersProperty::Unknown(LexUnknown { description: None }),
    }
}

/// Build schema from an enum (union)
pub fn build_enum_schema(input: &DeriveInput) -> syn::Result<BuiltSchema> {
    let type_attrs = parse_type_attrs(&input.attrs)?;
    let nsid = determine_nsid(&type_attrs, input)?;

    let data_enum = match &input.data {
        syn::Data::Enum(data_enum) => data_enum,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "build_enum_schema requires an enum",
            ));
        }
    };

    // Check if open union
    let is_open = has_open_union_attr(&input.attrs);

    // Extract variant refs
    let mut refs = Vec::new();
    for variant in &data_enum.variants {
        // Skip Unknown variant (added by #[open_union] macro)
        if variant.ident == "Unknown" {
            continue;
        }

        let variant_ref = extract_variant_ref(variant, &nsid)?;
        refs.push(variant_ref.into());
    }

    // Build union
    let user_type = LexUserType::Union(LexRefUnion {
        description: None,
        refs,
        closed: if is_open { None } else { Some(true) },
    });

    let mut defs = BTreeMap::new();
    defs.insert("main".into(), user_type);

    let doc = LexiconDoc {
        lexicon: Lexicon::Lexicon1,
        id: nsid.clone().into(),
        revision: None,
        description: None,
        defs,
    };

    // Unions don't have fragments in typical usage
    let schema_id = nsid.clone();

    Ok(BuiltSchema {
        nsid,
        schema_id,
        doc,
        validation_checks: Vec::new(), // Unions don't have validation
        unresolved_refs: Vec::new(),   // Union variants use explicit refs
        union_fields: BTreeMap::new(), // Unions don't have union fields
    })
}
