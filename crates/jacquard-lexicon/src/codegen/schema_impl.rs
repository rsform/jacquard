//! Generate LexiconSchema trait implementations for generated types

use crate::lexicon::{
    LexInteger, LexObject, LexObjectProperty, LexRecordRecord, LexString, LexUserType, LexiconDoc,
};
use crate::schema::from_ast::{ConstraintCheck, ValidationCheck};

/// Extract validation checks from a LexiconDoc
///
/// Walks the lexicon structure and builds ValidationCheck structs for all
/// constraint fields (max_length, max_graphemes, minimum, maximum, etc.)
pub(crate) fn extract_validation_checks(doc: &LexiconDoc, def_name: &str) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    // Get the specified def
    if let Some(def) = doc.defs.get(def_name) {
        match def {
            LexUserType::Record(rec) => match &rec.record {
                LexRecordRecord::Object(obj) => {
                    checks.extend(extract_object_validations(obj));
                }
            },
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
                    is_array: true,
                    check: ConstraintCheck::MaxLength { max },
                });
            }
            if let Some(min) = arr.min_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: "Vec<_>".to_string(),
                    is_required,
                    is_array: true,
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
            is_array: false,
            check: ConstraintCheck::MaxLength { max },
        });
    }

    if let Some(min) = string.min_length {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            is_array: false,
            check: ConstraintCheck::MinLength { min },
        });
    }

    if let Some(max) = string.max_graphemes {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            is_array: false,
            check: ConstraintCheck::MaxGraphemes { max },
        });
    }

    if let Some(min) = string.min_graphemes {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            is_array: false,
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
            is_array: false,
            check: ConstraintCheck::Maximum { max },
        });
    }

    if let Some(min) = integer.minimum {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "i64".to_string(),
            is_required,
            is_array: false,
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
