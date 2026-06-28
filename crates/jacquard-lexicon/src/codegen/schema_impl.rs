//! Generate LexiconSchema trait implementations for generated types

use crate::corpus::LexiconCorpus;
use crate::lexicon::{
    LexInteger, LexObject, LexObjectProperty, LexRecordRecord, LexString, LexStringFormat,
    LexUserType, LexiconDoc,
};
use crate::ref_utils::RefPath;
use crate::schema::from_ast::{ConstraintCheck, ValidationCheck};
use std::collections::BTreeSet;

/// Extract validation checks from a LexiconDoc
///
/// Walks the lexicon structure and builds ValidationCheck structs for all
/// constraint fields (max_length, max_graphemes, minimum, maximum, etc.)
pub(crate) fn extract_validation_checks(
    corpus: &LexiconCorpus,
    doc: &LexiconDoc,
    def_name: &str,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    // Get the specified def
    if let Some(def) = doc.defs.get(def_name) {
        match def {
            LexUserType::Record(rec) => match &rec.record {
                LexRecordRecord::Object(obj) => {
                    checks.extend(extract_object_validations(
                        obj,
                        corpus,
                        doc.id.as_ref(),
                        &mut BTreeSet::new(),
                    ));
                }
            },
            LexUserType::Object(obj) => {
                checks.extend(extract_object_validations(
                    obj,
                    corpus,
                    doc.id.as_ref(),
                    &mut BTreeSet::new(),
                ));
            }
            // XRPC types, tokens, etc. don't need validation
            _ => {}
        }
    }

    checks
}

/// Extract validation checks from an object's properties.
fn extract_object_validations(
    obj: &LexObject,
    corpus: &LexiconCorpus,
    current_nsid: &str,
    seen_refs: &mut BTreeSet<String>,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    for (schema_name, prop) in &obj.properties {
        let field_name = field_name_from_schema(schema_name);
        let is_required = obj
            .required
            .as_ref()
            .map(|req| req.iter().any(|r| r == schema_name))
            .unwrap_or(false);
        checks.extend(extract_property_validations(
            &field_name,
            schema_name.as_ref(),
            prop,
            is_required,
            false,
            corpus,
            current_nsid,
            seen_refs,
        ));
    }

    checks
}

/// Extract validation checks from a single property, following local refs.
fn extract_property_validations(
    field_name: &str,
    schema_name: &str,
    prop: &LexObjectProperty,
    is_required: bool,
    is_array_item: bool,
    corpus: &LexiconCorpus,
    current_nsid: &str,
    seen_refs: &mut BTreeSet<String>,
) -> Vec<ValidationCheck> {
    match prop {
        LexObjectProperty::String(s) => {
            extract_string_validations(field_name, schema_name, s, is_required, is_array_item)
        }
        LexObjectProperty::Integer(i) => {
            extract_integer_validations(field_name, schema_name, i, is_required, is_array_item)
        }
        LexObjectProperty::Array(arr) => {
            let mut checks = Vec::new();
            if let Some(max) = arr.max_length {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: "Vec<_>".to_string(),
                    is_required,
                    is_array: true,
                    is_array_item: false,
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
                    is_array_item: false,
                    check: ConstraintCheck::MinLength { min },
                });
            }
            checks.extend(extract_array_item_validations(
                field_name,
                schema_name,
                &arr.items,
                is_required,
                corpus,
                current_nsid,
                seen_refs,
            ));
            checks
        }
        LexObjectProperty::Blob(b) => {
            let mut checks = Vec::new();
            if let Some(max) = b.max_size {
                checks.push(ValidationCheck {
                    field_name: field_name.to_string(),
                    schema_name: schema_name.to_string(),
                    field_type: "BlobRef".to_string(),
                    is_required,
                    is_array: false,
                    is_array_item,
                    check: ConstraintCheck::BlobMaxSize { max },
                });
            }
            if let Some(accept) = &b.accept {
                if !accept.is_empty() {
                    checks.push(ValidationCheck {
                        field_name: field_name.to_string(),
                        schema_name: schema_name.to_string(),
                        field_type: "BlobRef".to_string(),
                        is_required,
                        is_array: false,
                        is_array_item,
                        check: ConstraintCheck::BlobAccept {
                            accept: accept.iter().map(|m| m.as_str().to_string()).collect(),
                        },
                    });
                }
            }
            checks
        }
        LexObjectProperty::Ref(r) => {
            let normalized_ref = RefPath::normalize(r.r#ref.as_ref(), current_nsid);
            if !seen_refs.insert(normalized_ref.clone()) {
                return Vec::new();
            }
            let Some((ref_doc, ref_def)) = corpus.resolve_ref(&normalized_ref) else {
                return Vec::new();
            };
            let checks = match ref_def {
                LexUserType::String(s) => extract_string_validations(
                    field_name,
                    schema_name,
                    s,
                    is_required,
                    is_array_item,
                ),
                LexUserType::Integer(i) => extract_integer_validations(
                    field_name,
                    schema_name,
                    i,
                    is_required,
                    is_array_item,
                ),
                LexUserType::Array(a) => extract_property_validations(
                    field_name,
                    schema_name,
                    &LexObjectProperty::Array(a.clone()),
                    is_required,
                    is_array_item,
                    corpus,
                    ref_doc.id.as_ref(),
                    seen_refs,
                ),
                _ => Vec::new(),
            };
            seen_refs.remove(&normalized_ref);
            checks
        }
        _ => Vec::new(),
    }
}

fn extract_array_item_validations(
    field_name: &str,
    schema_name: &str,
    item: &crate::lexicon::LexArrayItem,
    is_required: bool,
    corpus: &LexiconCorpus,
    current_nsid: &str,
    seen_refs: &mut BTreeSet<String>,
) -> Vec<ValidationCheck> {
    match item {
        crate::lexicon::LexArrayItem::String(s) => {
            extract_string_validations(field_name, schema_name, s, is_required, true)
        }
        crate::lexicon::LexArrayItem::Integer(i) => {
            extract_integer_validations(field_name, schema_name, i, is_required, true)
        }
        crate::lexicon::LexArrayItem::Ref(r) => extract_property_validations(
            field_name,
            schema_name,
            &LexObjectProperty::Ref(r.clone()),
            is_required,
            true,
            corpus,
            current_nsid,
            seen_refs,
        ),
        _ => Vec::new(),
    }
}

/// Extract validation checks from a string property
fn extract_string_validations(
    field_name: &str,
    schema_name: &str,
    string: &LexString,
    is_required: bool,
    is_array_item: bool,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    // Datetime maps to `chrono::DateTime<FixedOffset>` which does not implement
    // `AsRef<str>`, so length checks cannot be emitted for it. All other formats
    // (did, handle, at-uri, cid, nsid, tid, record-key, language, uri, etc.) use
    // string-backed wrapper types that do implement `AsRef<str>`.
    if matches!(string.format, Some(LexStringFormat::Datetime)) {
        return checks;
    }

    if let Some(max) = string.max_length {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "String".to_string(),
            is_required,
            is_array: false,
            is_array_item,
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
            is_array_item,
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
            is_array_item,
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
            is_array_item,
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
    is_array_item: bool,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    if let Some(max) = integer.maximum {
        checks.push(ValidationCheck {
            field_name: field_name.to_string(),
            schema_name: schema_name.to_string(),
            field_type: "i64".to_string(),
            is_required,
            is_array: false,
            is_array_item,
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
            is_array_item,
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
    use std::fs;

    #[test]
    fn follows_reused_cross_namespace_scalar_refs_and_array_items() {
        let directory = tempfile::tempdir().expect("create fixture directory");
        fs::write(
            directory.path().join("defs.json"),
            r#"{
                "lexicon": 1,
                "id": "test.shared.defs",
                "defs": {
                    "name": {
                        "type": "string",
                        "maxLength": 4,
                        "maxGraphemes": 4
                    }
                }
            }"#,
        )
        .expect("write shared defs");
        fs::write(
            directory.path().join("record.json"),
            r#"{
                "lexicon": 1,
                "id": "test.consumer.record",
                "defs": {
                    "main": {
                        "type": "record",
                        "record": {
                            "type": "object",
                            "required": ["name", "tags"],
                            "properties": {
                                "name": {
                                    "type": "ref",
                                    "ref": "test.shared.defs#name"
                                },
                                "tags": {
                                    "type": "array",
                                    "items": {
                                        "type": "ref",
                                        "ref": "test.shared.defs#name"
                                    }
                                }
                            }
                        }
                    }
                }
            }"#,
        )
        .expect("write consumer record");

        let corpus = LexiconCorpus::load_from_dir(directory.path()).expect("load fixture corpus");
        let doc = corpus
            .get("test.consumer.record")
            .expect("get consumer record");
        let checks = extract_validation_checks(&corpus, doc, "main");

        assert_eq!(
            checks
                .iter()
                .filter(|check| check.field_name == "name")
                .count(),
            2
        );
        assert_eq!(
            checks
                .iter()
                .filter(|check| check.field_name == "tags")
                .count(),
            2
        );
        assert!(
            checks
                .iter()
                .filter(|check| check.field_name == "tags")
                .all(|check| check.is_array_item)
        );
    }
}
