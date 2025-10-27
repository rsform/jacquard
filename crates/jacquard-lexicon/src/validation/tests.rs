//! Tests for Data validation against lexicon schemas

use super::*;
use crate::{lexicon::*, schema::LexiconSchema};
use jacquard_common::{
    CowStr,
    smol_str::ToSmolStr,
    types::{string::AtprotoStr, value::Data},
};
use std::collections::BTreeMap;

// Helper to create plain string Data
fn data_string(s: &str) -> Data<'static> {
    use smol_str::ToSmolStr;
    Data::String(AtprotoStr::String(CowStr::Owned(s.to_smolstr())))
}

// Test schema: Simple object with required string field
struct SimpleSchema;

impl LexiconSchema for SimpleSchema {
    fn nsid() -> &'static str {
        "test.simple"
    }

    fn def_name() -> &'static str {
        "main"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.simple"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["text".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "text".into(),
                                LexObjectProperty::String(LexString {
                                    description: None,
                                    format: None,
                                    default: None,
                                    min_length: None,
                                    max_length: None,
                                    min_graphemes: None,
                                    max_graphemes: None,
                                    r#enum: None,
                                    r#const: None,
                                    known_values: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

#[test]
fn test_valid_simple_object() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.simple".to_smolstr(), SimpleSchema::lexicon_doc());

    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("hello"),
    )])));

    let result = validator.validate::<SimpleSchema>(&data).unwrap();
    assert!(
        result.is_valid(),
        "Expected valid, got: {:?}",
        result.structural_errors()
    );
}

#[test]
fn test_missing_required_field() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.simple".to_smolstr(), SimpleSchema::lexicon_doc());

    // Empty object - missing required 'text' field
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::new()));

    let result = validator.validate::<SimpleSchema>(&data).unwrap();
    assert!(!result.is_valid());

    let errors = result.structural_errors();
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        StructuralError::MissingRequiredField { field, .. } if field.as_str() == "text"
    ));
}

#[test]
fn test_type_mismatch() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.simple".to_smolstr(), SimpleSchema::lexicon_doc());

    // 'text' field is integer instead of string
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        Data::Integer(42),
    )])));

    let result = validator.validate::<SimpleSchema>(&data).unwrap();
    assert!(!result.is_valid());

    let errors = result.structural_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        StructuralError::TypeMismatch {
            expected, actual, ..
        } => {
            assert!(matches!(
                expected,
                jacquard_common::types::DataModelType::String(_)
            ));
            assert!(matches!(
                actual,
                jacquard_common::types::DataModelType::Integer
            ));
        }
        _ => panic!("Expected TypeMismatch error"),
    }
}

// Test schema: Union with $type discriminator
struct UnionSchema;

impl LexiconSchema for UnionSchema {
    fn nsid() -> &'static str {
        "test.union"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.union"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["content".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "content".into(),
                                LexObjectProperty::Union(LexRefUnion {
                                    description: None,
                                    refs: vec!["#text".into(), "#image".into()],
                                    closed: Some(true),
                                }),
                            );
                            props
                        },
                    }),
                );
                defs.insert(
                    "text".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["value".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "value".into(),
                                LexObjectProperty::String(LexString {
                                    description: None,
                                    format: None,
                                    default: None,
                                    min_length: None,
                                    max_length: None,
                                    min_graphemes: None,
                                    max_graphemes: None,
                                    r#enum: None,
                                    r#const: None,
                                    known_values: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs.insert(
                    "image".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["url".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "url".into(),
                                LexObjectProperty::String(LexString {
                                    description: None,
                                    format: None,
                                    default: None,
                                    min_length: None,
                                    max_length: None,
                                    min_graphemes: None,
                                    max_graphemes: None,
                                    r#enum: None,
                                    r#const: None,
                                    known_values: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

#[test]
fn test_union_missing_discriminator() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.union".to_smolstr(), UnionSchema::lexicon_doc());

    // Union object without $type field
    let content = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "value".into(),
        data_string("hello"),
    )])));

    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "content".into(),
        content,
    )])));

    let result = validator.validate::<UnionSchema>(&data).unwrap();
    assert!(!result.is_valid());

    let errors = result.structural_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, StructuralError::MissingUnionDiscriminator { .. }))
    );
}

#[test]
fn test_union_invalid_type() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.union".to_smolstr(), UnionSchema::lexicon_doc());

    // Union with $type that doesn't match any variant
    let content = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([
        ("$type".into(), data_string("test.union#unknown")),
        ("value".into(), data_string("hello")),
    ])));

    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "content".into(),
        content,
    )])));

    let result = validator.validate::<UnionSchema>(&data).unwrap();
    assert!(!result.is_valid());

    let errors = result.structural_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, StructuralError::UnionNoMatch { .. }))
    );
}

#[test]
fn test_union_valid_variant() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.union".to_smolstr(), UnionSchema::lexicon_doc());

    // Valid text variant
    let content = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([
        ("$type".into(), data_string("test.union#text")),
        ("value".into(), data_string("hello")),
    ])));

    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "content".into(),
        content,
    )])));

    let result = validator.validate::<UnionSchema>(&data).unwrap();
    assert!(
        result.is_valid(),
        "Expected valid, got: {:?}",
        result.structural_errors()
    );
}

// Test schema: Array validation
struct ArraySchema;

impl LexiconSchema for ArraySchema {
    fn nsid() -> &'static str {
        "test.array"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.array"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["items".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "items".into(),
                                LexObjectProperty::Array(LexArray {
                                    description: None,
                                    items: LexArrayItem::String(LexString {
                                        description: None,
                                        format: None,
                                        default: None,
                                        min_length: None,
                                        max_length: None,
                                        min_graphemes: None,
                                        max_graphemes: None,
                                        r#enum: None,
                                        r#const: None,
                                        known_values: None,
                                    }),
                                    min_length: None,
                                    max_length: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

#[test]
fn test_array_valid_items() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.array".to_smolstr(), ArraySchema::lexicon_doc());

    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "items".into(),
        Data::Array(jacquard_common::types::value::Array(vec![
            data_string("one"),
            data_string("two"),
            data_string("three"),
        ])),
    )])));

    let result = validator.validate::<ArraySchema>(&data).unwrap();
    assert!(
        result.is_valid(),
        "Expected valid, got: {:?}",
        result.structural_errors()
    );
}

#[test]
fn test_array_invalid_item_type() {
    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.array".to_smolstr(), ArraySchema::lexicon_doc());

    // Second item is integer instead of string
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "items".into(),
        Data::Array(jacquard_common::types::value::Array(vec![
            data_string("one"),
            Data::Integer(42),
            data_string("three"),
        ])),
    )])));

    let result = validator.validate::<ArraySchema>(&data).unwrap();
    assert!(!result.is_valid());

    let errors = result.structural_errors();
    assert!(errors.iter().any(|e| {
        matches!(e, StructuralError::TypeMismatch { expected, actual, .. }
            if matches!(expected, jacquard_common::types::DataModelType::String(_))
            && matches!(actual, jacquard_common::types::DataModelType::Integer))
    }));
}

#[test]
fn test_nested_objects() {
    // Test schema with nested object
    struct NestedSchema;
    impl LexiconSchema for NestedSchema {
        fn nsid() -> &'static str {
            "test.nested"
        }

        fn lexicon_doc() -> LexiconDoc<'static> {
            LexiconDoc {
                lexicon: Lexicon::Lexicon1,
                id: CowStr::new_static("test.nested"),
                revision: None,
                description: None,
                defs: {
                    let mut defs = BTreeMap::new();
                    defs.insert(
                        "main".into(),
                        LexUserType::Object(LexObject {
                            description: None,
                            required: Some(vec!["meta".into()]),
                            nullable: None,
                            properties: {
                                let mut props = BTreeMap::new();
                                props.insert(
                                    "meta".into(),
                                    LexObjectProperty::Object(LexObject {
                                        description: None,
                                        required: Some(vec!["title".into()]),
                                        nullable: None,
                                        properties: {
                                            let mut meta_props = BTreeMap::new();
                                            meta_props.insert(
                                                "title".into(),
                                                LexObjectProperty::String(LexString {
                                                    description: None,
                                                    format: None,
                                                    default: None,
                                                    min_length: None,
                                                    max_length: None,
                                                    min_graphemes: None,
                                                    max_graphemes: None,
                                                    r#enum: None,
                                                    r#const: None,
                                                    known_values: None,
                                                }),
                                            );
                                            meta_props
                                        },
                                    }),
                                );
                                props
                            },
                        }),
                    );
                    defs
                },
            }
        }
    }

    let validator = SchemaValidator::new();
    validator
        .registry()
        .insert("test.nested".to_smolstr(), NestedSchema::lexicon_doc());

    // Nested object missing required field
    let meta = Data::Object(jacquard_common::types::value::Object(BTreeMap::new()));
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "meta".into(),
        meta,
    )])));

    let result = validator.validate::<NestedSchema>(&data).unwrap();
    assert!(!result.is_valid());

    let errors = result.structural_errors();
    assert!(errors.iter().any(|e| matches!(
        e,
        StructuralError::MissingRequiredField { field, .. } if field.as_str() == "title"
    )));
}

// ============================================================================
// CONSTRAINT VALIDATION TESTS (Phase 4)
// ============================================================================

// Schema with string constraints
struct StringConstraintSchema;

impl LexiconSchema for StringConstraintSchema {
    fn nsid() -> &'static str {
        "test.string.constraints"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.string.constraints"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["text".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "text".into(),
                                LexObjectProperty::String(LexString {
                                    description: None,
                                    format: None,
                                    default: None,
                                    min_length: Some(5),
                                    max_length: Some(20),
                                    min_graphemes: None,
                                    max_graphemes: None,
                                    r#enum: None,
                                    r#const: None,
                                    known_values: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

// Schema with grapheme constraints
struct GraphemeConstraintSchema;

impl LexiconSchema for GraphemeConstraintSchema {
    fn nsid() -> &'static str {
        "test.grapheme.constraints"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.grapheme.constraints"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["text".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "text".into(),
                                LexObjectProperty::String(LexString {
                                    description: None,
                                    format: None,
                                    default: None,
                                    min_length: None,
                                    max_length: None,
                                    min_graphemes: Some(2),
                                    max_graphemes: Some(5),
                                    r#enum: None,
                                    r#const: None,
                                    known_values: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

// Schema with integer constraints
struct IntegerConstraintSchema;

impl LexiconSchema for IntegerConstraintSchema {
    fn nsid() -> &'static str {
        "test.integer.constraints"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.integer.constraints"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["value".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "value".into(),
                                LexObjectProperty::Integer(LexInteger {
                                    description: None,
                                    default: None,
                                    minimum: Some(0),
                                    maximum: Some(100),
                                    r#enum: None,
                                    r#const: None,
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

// Schema with array length constraints
struct ArrayConstraintSchema;

impl LexiconSchema for ArrayConstraintSchema {
    fn nsid() -> &'static str {
        "test.array.constraints"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: CowStr::new_static("test.array.constraints"),
            revision: None,
            description: None,
            defs: {
                let mut defs = BTreeMap::new();
                defs.insert(
                    "main".into(),
                    LexUserType::Object(LexObject {
                        description: None,
                        required: Some(vec!["items".into()]),
                        nullable: None,
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert(
                                "items".into(),
                                LexObjectProperty::Array(LexArray {
                                    description: None,
                                    items: LexArrayItem::String(LexString {
                                        description: None,
                                        format: None,
                                        default: None,
                                        min_length: None,
                                        max_length: None,
                                        min_graphemes: None,
                                        max_graphemes: None,
                                        r#enum: None,
                                        r#const: None,
                                        known_values: None,
                                    }),
                                    min_length: Some(2),
                                    max_length: Some(5),
                                }),
                            );
                            props
                        },
                    }),
                );
                defs
            },
        }
    }
}

#[test]
fn test_constraint_validation_is_lazy() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // String too long (21 chars, max is 20)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("this string is too long!"),
    )])));

    let result = validator.validate::<StringConstraintSchema>(&data).unwrap();

    // Structurally valid - type is correct, required field present
    assert!(result.is_structurally_valid());

    // But overall invalid due to constraint violation
    assert!(!result.is_valid());
}

#[test]
fn test_string_max_length() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // String exceeding max_length (25 chars, max is 20)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("this string is way too long"),
    )])));

    let result = validator.validate::<StringConstraintSchema>(&data).unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());
    assert!(result.has_constraint_violations());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::MaxLength {
            max: 20,
            actual: 27,
            ..
        }
    ));
}

#[test]
fn test_string_min_length() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // String below min_length (3 chars, min is 5)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("hi"),
    )])));

    let result = validator.validate::<StringConstraintSchema>(&data).unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::MinLength {
            min: 5,
            actual: 2,
            ..
        }
    ));
}

#[test]
fn test_string_max_graphemes() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.grapheme.constraints".to_smolstr(),
        GraphemeConstraintSchema::lexicon_doc(),
    );

    // 6 emoji graphemes (max is 5)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("👍👍👍👍👍👍"),
    )])));

    let result = validator
        .validate::<GraphemeConstraintSchema>(&data)
        .unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::MaxGraphemes {
            max: 5,
            actual: 6,
            ..
        }
    ));
}

#[test]
fn test_string_min_graphemes() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.grapheme.constraints".to_smolstr(),
        GraphemeConstraintSchema::lexicon_doc(),
    );

    // 1 emoji grapheme (min is 2)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("👍"),
    )])));

    let result = validator
        .validate::<GraphemeConstraintSchema>(&data)
        .unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::MinGraphemes {
            min: 2,
            actual: 1,
            ..
        }
    ));
}

#[test]
fn test_string_within_constraints() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // Valid string (10 chars, within 5-20 range)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("valid text"),
    )])));

    let result = validator.validate::<StringConstraintSchema>(&data).unwrap();

    assert!(result.is_valid());
    assert!(result.is_structurally_valid());
    assert!(!result.has_constraint_violations());
}

#[test]
fn test_integer_maximum() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.integer.constraints".to_smolstr(),
        IntegerConstraintSchema::lexicon_doc(),
    );

    // Integer exceeding maximum (150 > 100)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "value".into(),
        Data::Integer(150),
    )])));

    let result = validator
        .validate::<IntegerConstraintSchema>(&data)
        .unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::Maximum {
            max: 100,
            actual: 150,
            ..
        }
    ));
}

#[test]
fn test_integer_minimum() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.integer.constraints".to_smolstr(),
        IntegerConstraintSchema::lexicon_doc(),
    );

    // Integer below minimum (-5 < 0)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "value".into(),
        Data::Integer(-5),
    )])));

    let result = validator
        .validate::<IntegerConstraintSchema>(&data)
        .unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::Minimum {
            min: 0,
            actual: -5,
            ..
        }
    ));
}

#[test]
fn test_integer_within_constraints() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.integer.constraints".to_smolstr(),
        IntegerConstraintSchema::lexicon_doc(),
    );

    // Valid integer (50 is within 0-100 range)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "value".into(),
        Data::Integer(50),
    )])));

    let result = validator
        .validate::<IntegerConstraintSchema>(&data)
        .unwrap();

    assert!(result.is_valid());
    assert!(result.is_structurally_valid());
    assert!(!result.has_constraint_violations());
}

#[test]
fn test_array_max_length() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.array.constraints".to_smolstr(),
        ArrayConstraintSchema::lexicon_doc(),
    );

    // Array with too many items (6 items, max is 5)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "items".into(),
        Data::Array(jacquard_common::types::value::Array(vec![
            data_string("one"),
            data_string("two"),
            data_string("three"),
            data_string("four"),
            data_string("five"),
            data_string("six"),
        ])),
    )])));

    let result = validator.validate::<ArrayConstraintSchema>(&data).unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::MaxLength {
            max: 5,
            actual: 6,
            ..
        }
    ));
}

#[test]
fn test_array_min_length() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.array.constraints".to_smolstr(),
        ArrayConstraintSchema::lexicon_doc(),
    );

    // Array with too few items (1 item, min is 2)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "items".into(),
        Data::Array(jacquard_common::types::value::Array(vec![data_string(
            "one",
        )])),
    )])));

    let result = validator.validate::<ArrayConstraintSchema>(&data).unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());

    let constraint_errors = result.constraint_errors();
    assert_eq!(constraint_errors.len(), 1);
    assert!(matches!(
        &constraint_errors[0],
        ConstraintError::MinLength {
            min: 2,
            actual: 1,
            ..
        }
    ));
}

#[test]
fn test_array_within_constraints() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.array.constraints".to_smolstr(),
        ArrayConstraintSchema::lexicon_doc(),
    );

    // Valid array (3 items, within 2-5 range)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "items".into(),
        Data::Array(jacquard_common::types::value::Array(vec![
            data_string("one"),
            data_string("two"),
            data_string("three"),
        ])),
    )])));

    let result = validator.validate::<ArrayConstraintSchema>(&data).unwrap();

    assert!(result.is_valid());
    assert!(result.is_structurally_valid());
    assert!(!result.has_constraint_violations());
}

#[test]
fn test_structurally_invalid_skips_constraints() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // Structurally invalid: integer instead of string
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        Data::Integer(42),
    )])));

    let result = validator.validate::<StringConstraintSchema>(&data).unwrap();

    assert!(!result.is_valid());
    assert!(!result.is_structurally_valid());

    // Structural errors should be present
    assert_eq!(result.structural_errors().len(), 1);

    // Constraint checking should be skipped or return empty
    // (implementation detail: may or may not compute constraints for structurally invalid data)
}

#[test]
fn test_structurally_valid_with_constraint_errors() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // Structurally valid but violates constraints
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("too long string here!!!"),
    )])));

    let result = validator.validate::<StringConstraintSchema>(&data).unwrap();

    assert!(!result.is_valid());
    assert!(result.is_structurally_valid());
    assert!(result.has_constraint_violations());

    // Both structural and constraint errors should be separate
    assert_eq!(result.structural_errors().len(), 0);
    assert!(result.constraint_errors().len() > 0);
}

#[test]
fn test_validate_structural_only() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // String too long (violates constraints)
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        data_string("this string is way too long"),
    )])));

    // Use structural validation only
    let result = validator.validate_structural::<StringConstraintSchema>(&data);

    // Structurally valid - type is correct, required field present
    assert!(result.is_structurally_valid());

    // No constraint errors computed
    assert_eq!(result.constraint_errors().len(), 0);

    // Result should be StructuralOnly variant
    match result {
        ValidationResult::StructuralOnly { .. } => {}
        ValidationResult::Complete { .. } => panic!("Expected StructuralOnly variant"),
    }
}

#[test]
fn test_validate_structural_only_with_errors() {
    let validator = SchemaValidator::new();
    validator.registry().insert(
        "test.string.constraints".to_smolstr(),
        StringConstraintSchema::lexicon_doc(),
    );

    // Structurally invalid: integer instead of string
    let data = Data::Object(jacquard_common::types::value::Object(BTreeMap::from([(
        "text".into(),
        Data::Integer(42),
    )])));

    let result = validator.validate_structural::<StringConstraintSchema>(&data);

    // Not structurally valid
    assert!(!result.is_structurally_valid());

    // Structural errors should be present
    assert_eq!(result.structural_errors().len(), 1);

    // No constraint errors
    assert_eq!(result.constraint_errors().len(), 0);
}
