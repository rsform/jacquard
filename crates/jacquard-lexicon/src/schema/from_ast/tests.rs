// Unit tests for schema generation from Rust AST
//
// These tests verify that build_struct_schema() generates correct lexicon documents
// and validation checks from Rust type definitions

use super::*;
use crate::lexicon::*;
use syn::parse_quote;

#[test]
fn test_simple_struct_with_string_constraint() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.simple")]
        struct SimplePost<'a> {
            #[lexicon(max_length = 300)]
            text: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    // Check NSID
    assert_eq!(result.nsid, "test.simple");
    assert_eq!(result.schema_id, "test.simple");

    // Check generated doc structure
    assert_eq!(result.doc.id.as_ref(), "test.simple");
    let main_def = result.doc.defs.get("main").expect("has main def");

    if let LexUserType::Object(obj) = main_def {
        // Check text property
        let text_prop = obj.properties.get("text").expect("has text property");
        if let LexObjectProperty::String(s) = text_prop {
            assert_eq!(s.max_length, Some(300));
        } else {
            panic!("text should be string property");
        }

        // Check required fields
        assert_eq!(obj.required, Some(vec!["text".into()]));
    } else {
        panic!("main def should be object");
    }

    // Check validation
    assert_eq!(result.validation_checks.len(), 1);
    let check = &result.validation_checks[0];
    assert_eq!(check.field_name, "text");
    assert_eq!(check.schema_name, "text");
    assert!(matches!(
        check.check,
        ConstraintCheck::MaxLength { max: 300 }
    ));
}

#[test]
fn test_integer_constraints() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.integer")]
        struct Profile {
            #[lexicon(minimum = 13, maximum = 120)]
            age: i64,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let age_prop = obj.properties.get("age").expect("has age");
        if let LexObjectProperty::Integer(i) = age_prop {
            assert_eq!(i.minimum, Some(13));
            assert_eq!(i.maximum, Some(120));
        } else {
            panic!("age should be integer");
        }
    }

    // Check both constraints
    assert_eq!(result.validation_checks.len(), 2);
    assert!(
        result
            .validation_checks
            .iter()
            .any(|c| matches!(&c.check, ConstraintCheck::Minimum { min: 13 }))
    );
    assert!(
        result
            .validation_checks
            .iter()
            .any(|c| matches!(&c.check, ConstraintCheck::Maximum { max: 120 }))
    );
}

#[test]
fn test_grapheme_constraints() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.graphemes")]
        struct Bio<'a> {
            #[lexicon(min_graphemes = 1, max_graphemes = 256)]
            bio: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let bio_prop = obj.properties.get("bio").expect("has bio");
        if let LexObjectProperty::String(s) = bio_prop {
            assert_eq!(s.min_graphemes, Some(1));
            assert_eq!(s.max_graphemes, Some(256));
        } else {
            panic!("bio should be string");
        }
    }

    assert_eq!(result.validation_checks.len(), 2);
}

#[test]
fn test_optional_vs_required_fields() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.optional")]
        struct Fields<'a> {
            required: CowStr<'a>,
            optional: Option<CowStr<'a>>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        assert!(obj.properties.contains_key("required"));
        assert!(obj.properties.contains_key("optional"));

        // Only required should be in required list
        if let Some(required) = &obj.required {
            assert!(required.contains(&"required".into()));
            assert!(!required.contains(&"optional".into()));
        } else {
            panic!("should have required list");
        }
    }
}

#[test]
fn test_serde_rename_all_camelcase() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.rename")]
        #[serde(rename_all = "camelCase")]
        struct RenamedFields<'a> {
            my_field: CowStr<'a>,
            another_field: i64,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        // Fields should be camelCase in schema
        assert!(obj.properties.contains_key("myField"));
        assert!(obj.properties.contains_key("anotherField"));
        assert!(!obj.properties.contains_key("my_field"));
    }
}

#[test]
fn test_serde_rename_individual_field() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.individual")]
        struct Renamed<'a> {
            #[serde(rename = "customName")]
            my_field: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        assert!(obj.properties.contains_key("customName"));
        assert!(!obj.properties.contains_key("my_field"));
    }
}

#[test]
fn test_array_constraints() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.arrays")]
        struct Tagged<'a> {
            #[lexicon(max_items = 10)]
            tags: Vec<CowStr<'a>>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let tags_prop = obj.properties.get("tags").expect("has tags");
        if let LexObjectProperty::Array(arr) = tags_prop {
            assert_eq!(arr.max_length, Some(10));
        } else {
            panic!("tags should be array");
        }
    }
}

#[test]
fn test_record_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.record", record, key = "tid")]
        struct Post<'a> {
            text: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    // Should be Record, not Object
    if let LexUserType::Record(record) = main_def {
        assert_eq!(record.key, Some("tid".into()));

        // Record should have nested object
        match &record.record {
            crate::lexicon::LexRecordRecord::Object(obj) => {
                assert!(obj.properties.contains_key("text"));
            }
        }
    } else {
        panic!("should be record type");
    }
}

#[test]
fn test_fragment() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.multi", fragment = "option")]
        struct MultiOption<'a> {
            text: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    // NSID should not include fragment
    assert_eq!(result.nsid, "test.multi");
    // Schema ID should include fragment
    assert_eq!(result.schema_id, "test.multi#option");
}

#[test]
fn test_bytes_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.bytes")]
        struct Binary {
            #[lexicon(max_length = 5000)]
            data: bytes::Bytes,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let data_prop = obj.properties.get("data").expect("has data");
        if let LexObjectProperty::Bytes(b) = data_prop {
            assert_eq!(b.max_length, Some(5000));
        } else {
            panic!("data should be bytes");
        }
    }
}

#[test]
fn test_blob_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.blob")]
        struct Image<'a> {
            avatar: jacquard_common::types::blob::BlobRef<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let avatar_prop = obj.properties.get("avatar").expect("has avatar");
        // BlobRef might generate Ref instead of Blob
        assert!(matches!(avatar_prop, LexObjectProperty::Blob(_)));
    }
}

#[test]
fn test_cid_link_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.cid")]
        struct WithCid<'a> {
            content_id: jacquard_common::types::cid::CidLink<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let cid_prop = obj.properties.get("contentId").expect("has contentId");
        assert!(matches!(cid_prop, LexObjectProperty::CidLink(_)));
    }
}

#[test]
fn test_datetime_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.datetime")]
        struct Event {
            created_at: jacquard_common::types::string::Datetime,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let dt_prop = obj.properties.get("createdAt").expect("has createdAt");
        if let LexObjectProperty::String(s) = dt_prop {
            assert_eq!(s.format, Some(LexStringFormat::Datetime));
        } else {
            panic!("createdAt should be string with datetime format");
        }
    }
}

#[test]
fn test_did_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.did")]
        struct Actor<'a> {
            did: jacquard_common::types::string::Did<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let did_prop = obj.properties.get("did").expect("has did");
        if let LexObjectProperty::String(s) = did_prop {
            assert_eq!(s.format, Some(LexStringFormat::Did));
        } else {
            panic!("did should be string with did format");
        }
    }
}

#[test]
fn test_at_uri_type() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.uri")]
        struct Reference<'a> {
            uri: jacquard_common::types::string::AtUri<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let uri_prop = obj.properties.get("uri").expect("has uri");
        if let LexObjectProperty::String(s) = uri_prop {
            assert_eq!(s.format, Some(LexStringFormat::AtUri));
        } else {
            panic!("uri should be string with at-uri format");
        }
    }
}

#[test]
fn test_multiple_constraints_on_one_field() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.multiple")]
        struct Constrained<'a> {
            #[lexicon(min_length = 3, max_length = 20, min_graphemes = 2, max_graphemes = 18)]
            username: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let username_prop = obj.properties.get("username").expect("has username");
        if let LexObjectProperty::String(s) = username_prop {
            assert_eq!(s.min_length, Some(3));
            assert_eq!(s.max_length, Some(20));
            assert_eq!(s.min_graphemes, Some(2));
            assert_eq!(s.max_graphemes, Some(18));
        } else {
            panic!("username should be string");
        }
    }

    // Should have 4 validation checks
    assert_eq!(result.validation_checks.len(), 4);
}

#[test]
fn test_boolean_field() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.bool")]
        struct Flags {
            enabled: bool,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let enabled_prop = obj.properties.get("enabled").expect("has enabled");
        assert!(matches!(enabled_prop, LexObjectProperty::Boolean(_)));
    }
}

#[test]
fn test_f64_field() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.float")]
        struct Location {
            latitude: f64,
            longitude: f64,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        assert!(obj.properties.contains_key("latitude"));
        assert!(obj.properties.contains_key("longitude"));
    }
}

#[test]
#[ignore] // we're changing how this works
fn test_nested_object_generates_ref() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.nested")]
        struct Outer {
            inner: Inner,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let inner_prop = obj.properties.get("inner").expect("has inner");
        // Nested structs should generate Ref or Object depending on implementation
        assert!(matches!(
            inner_prop,
            LexObjectProperty::Ref(_) | LexObjectProperty::Object(_)
        ));
    }

    // Should have unresolved ref for Inner type
    assert!(!result.unresolved_refs.is_empty());
}

#[test]
fn test_explicit_ref_attribute() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.explicit")]
        struct Post<'a> {
            #[lexicon(ref = "com.atproto.repo.strongRef")]
            reference: SomeType<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let ref_prop = obj.properties.get("reference").expect("has reference");
        if let LexObjectProperty::Ref(r) = ref_prop {
            assert_eq!(r.r#ref.as_ref(), "com.atproto.repo.strongRef");
        } else {
            panic!("reference should be ref property");
        }
    }
}

#[test]
fn test_format_attribute() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.format")]
        struct Custom<'a> {
            #[lexicon(format = "nsid")]
            collection: CowStr<'a>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        let coll_prop = obj.properties.get("collection").expect("has collection");
        if let LexObjectProperty::String(s) = coll_prop {
            assert_eq!(s.format, Some(LexStringFormat::Nsid));
        } else {
            panic!("collection should be string");
        }
    }
}

#[test]
fn test_serde_skip_field() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.skip")]
        struct WithSkip<'a> {
            included: CowStr<'a>,
            #[serde(skip)]
            excluded: i64,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        assert!(obj.properties.contains_key("included"));
        assert!(!obj.properties.contains_key("excluded"));
    }
}

#[test]
fn test_serde_skip_serializing_if() {
    let input: syn::DeriveInput = parse_quote! {
        #[lexicon(nsid = "test.skipif")]
        struct WithSkipIf<'a> {
            required: CowStr<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            optional: Option<i64>,
        }
    };

    let result = build_struct_schema(&input).expect("build schema");

    let main_def = result.doc.defs.get("main").expect("has main");
    if let LexUserType::Object(obj) = main_def {
        assert!(obj.properties.contains_key("required"));
        assert!(obj.properties.contains_key("optional"));
    }
}
