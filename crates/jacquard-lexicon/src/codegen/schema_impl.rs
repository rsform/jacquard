//! Generate LexiconSchema trait implementations for generated types

use crate::derive_impl::doc_to_tokens;
use crate::lexicon::{
    LexArrayItem, LexInteger, LexObject, LexObjectProperty, LexRecord, LexRecordRecord, LexString,
    LexUserType, LexiconDoc,
};
use crate::schema::from_ast::{ConstraintCheck, ValidationCheck};
use proc_macro2::TokenStream;
use quote::quote;

/// Generate LexiconSchema impl for a generated type
///
/// Takes the original lexicon doc and type metadata to generate a complete
/// impl with const literal and validation code.
pub fn generate_schema_impl(
    type_name: &str,
    doc: &LexiconDoc,
    def_name: &str,
    has_lifetime: bool,
) -> TokenStream {
    let nsid = doc.id.as_ref();

    // Generate lifetime parameter
    let (impl_generics, type_generics) = if has_lifetime {
        (quote! { <'a> }, quote! { <'a> })
    } else {
        (quote! {}, quote! {})
    };

    // Generate the lexicon doc literal using existing doc_to_tokens
    let doc_literal = doc_to_tokens::doc_to_tokens(doc);

    // Extract validation checks from lexicon doc for the specific def
    let validation_checks = extract_validation_checks(doc, def_name);

    // Generate validation code using existing validations_to_tokens
    let validation_code = doc_to_tokens::validations_to_tokens(&validation_checks);

    let type_ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

    quote! {
        impl #impl_generics ::jacquard_lexicon::schema::LexiconSchema for #type_ident #type_generics {
            fn nsid() -> &'static str {
                #nsid
            }

            fn lexicon_doc(
                _generator: &mut ::jacquard_lexicon::schema::LexiconGenerator
            ) -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                #doc_literal
            }

            fn validate(&self) -> ::std::result::Result<(), ::jacquard_lexicon::schema::ValidationError> {
                #validation_code
            }
        }
    }
}

/// Extract validation checks from a LexiconDoc
///
/// Walks the lexicon structure and builds ValidationCheck structs for all
/// constraint fields (max_length, max_graphemes, minimum, maximum, etc.)
fn extract_validation_checks(doc: &LexiconDoc, def_name: &str) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    // Get the specified def
    if let Some(def) = doc.defs.get(def_name) {
        match def {
            LexUserType::Record(rec) => {
                match &rec.record {
                    LexRecordRecord::Object(obj) => {
                        checks.extend(extract_object_validations(obj));
                    }
                }
            }
            LexUserType::Object(obj) => {
                checks.extend(extract_object_validations(obj));
            }
            // XRPC types, tokens, etc. don't need validation
            _ => {}
        }
    }

    checks
}

/// Extract validation checks from an object's properties
fn extract_object_validations(obj: &LexObject) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    for (schema_name, prop) in &obj.properties {
        // Convert schema name to field name (snake_case, with r# prefix for keywords)
        let field_name = field_name_from_schema(schema_name);

        // Check if required
        let is_required = obj
            .required
            .as_ref()
            .map(|req| req.iter().any(|r| r == schema_name))
            .unwrap_or(false);

        // Extract checks from property
        checks.extend(extract_property_validations(
            &field_name,
            schema_name.as_ref(),
            prop,
            is_required,
        ));
    }

    checks
}

/// Extract validation checks from a single property
fn extract_property_validations(
    field_name: &str,
    schema_name: &str,
    prop: &LexObjectProperty,
    is_required: bool,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    match prop {
        LexObjectProperty::String(s) => {
            checks.extend(extract_string_validations(
                field_name,
                schema_name,
                s,
                is_required,
            ));
        }
        LexObjectProperty::Integer(i) => {
            checks.extend(extract_integer_validations(
                field_name,
                schema_name,
                i,
                is_required,
            ));
        }
        LexObjectProperty::Array(arr) => {
            if let Some(max) = arr.max_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: "Vec<_>".to_string(),
                    is_required,
                    check: ConstraintCheck::MaxLength { max },
                });
            }
            if let Some(min) = arr.min_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: "Vec<_>".to_string(),
                    is_required,
                    check: ConstraintCheck::MinLength { min },
                });
            }
        }
        _ => {
            // Other types don't have runtime validations in the current impl
        }
    }

    checks
}

/// Extract validation checks from a string property
fn extract_string_validations(
    field_name: &str,
    schema_name: &str,
    string: &LexString,
    is_required: bool,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    if let Some(max) = string.max_length {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            check: ConstraintCheck::MaxLength { max },
        });
    }

    if let Some(min) = string.min_length {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            check: ConstraintCheck::MinLength { min },
        });
    }

    if let Some(max) = string.max_graphemes {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            check: ConstraintCheck::MaxGraphemes { max },
        });
    }

    if let Some(min) = string.min_graphemes {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            check: ConstraintCheck::MinGraphemes { min },
        });
    }

    checks
}

/// Extract validation checks from an integer property
fn extract_integer_validations(
    field_name: &str,
    schema_name: &str,
    integer: &LexInteger,
    is_required: bool,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    if let Some(max) = integer.maximum {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "i64".to_string(),
            is_required,
            check: ConstraintCheck::Maximum { max },
        });
    }

    if let Some(min) = integer.minimum {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "i64".to_string(),
            is_required,
            check: ConstraintCheck::Minimum { min },
        });
    }

    checks
}

/// Convert schema field name to the Rust field identifier
///
/// Returns snake_case field name without r# prefix
/// (the r# will be added by make_ident when generating tokens)
fn field_name_from_schema(schema_name: &str) -> String {
    use heck::ToSnakeCase;
    schema_name.to_snake_case()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_name_from_schema() {
        assert_eq!(field_name_from_schema("createdAt"), "created_at");
        assert_eq!(field_name_from_schema("maxLength"), "max_length");
        assert_eq!(field_name_from_schema("text"), "text");
        assert_eq!(field_name_from_schema("ref"), "ref"); // r# added by make_ident later
        assert_eq!(field_name_from_schema("type"), "type"); // r# added by make_ident later
    }
}
