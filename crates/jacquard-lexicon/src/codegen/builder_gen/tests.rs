//! Tests for builder generation

use super::common::generate_common_types;
use super::state_mod::{RequiredField, collect_required_fields, generate_state_module};
use super::{BuilderGenContext, BuilderSchema};
use crate::codegen::CodeGenerator;
use crate::codegen::builder_gen::build_method;
use crate::corpus::LexiconCorpus;
use crate::lexicon::{
    LexInteger, LexObject, LexObjectProperty, LexString, LexXrpcParameters,
    LexXrpcParametersProperty,
};
use jacquard_common::smol_str::SmolStr;
use std::collections::BTreeMap;

#[test]
fn test_common_types_generation() {
    let tokens = generate_common_types();
    let code = tokens.to_string();

    // Verify key types are present
    assert!(code.contains("struct Set"));
    assert!(code.contains("struct Unset"));
    assert!(code.contains("trait IsSet"));
    assert!(code.contains("trait IsUnset"));
    assert!(code.contains("fn into_inner"));

    // Verify sealed trait pattern
    assert!(code.contains("mod private"));
    assert!(code.contains("trait Sealed"));

    // Verify it parses as valid Rust
    let _parsed: syn::File = syn::parse2(tokens).expect("Generated code should parse");
}

// TODO: re-enable these tests once i have time to get them to properly check and not be order-dependent
// #[test]
// fn test_collect_required_fields_object() {
//     let obj = LexObject {
//         description: None,
//         required: Some(vec![
//             SmolStr::new_static("foo"),
//             SmolStr::new_static("barBaz"),
//         ]),
//         nullable: None,
//         properties: Default::default(),
//     };

//     let schema = BuilderSchema::Object(&obj);
//     let fields = collect_required_fields(&schema);

//     assert_eq!(fields.len(), 2);
//     assert_eq!(fields[0].name_snake, "foo");
//     assert_eq!(fields[0].name_pascal, "Foo");
//     assert_eq!(fields[1].name_snake, "bar_baz");
//     assert_eq!(fields[1].name_pascal, "BarBaz");
// }

// #[test]
// fn test_collect_required_fields_parameters() {
//     let params = LexXrpcParameters {
//         description: None,
//         required: Some(vec![
//             SmolStr::new_static("limit"),
//             SmolStr::new_static("cursor"),
//         ]),
//         properties: Default::default(),
//     };

//     let schema = BuilderSchema::Parameters(&params);
//     let fields = collect_required_fields(&schema);

//     assert_eq!(fields.len(), 2);
//     assert_eq!(fields[1].name_snake, "limit");
//     assert_eq!(fields[1].name_pascal, "Limit");
//     assert_eq!(fields[0].name_snake, "cursor");
//     assert_eq!(fields[0].name_pascal, "Cursor");
// }

#[test]
fn test_state_module_generation() {
    let fields = vec![
        RequiredField::new("collection"),
        RequiredField::new("record"),
        RequiredField::new("repo"),
    ];

    let tokens = generate_state_module("CreateRecord", &fields);
    let code = tokens.to_string();

    // Verify module structure
    assert!(code.contains("pub mod create_record_state"));
    assert!(code.contains("pub trait State"));
    assert!(code.contains("pub struct Empty"));

    // Verify associated types
    assert!(code.contains("type Collection"));
    assert!(code.contains("type Record"));
    assert!(code.contains("type Repo"));

    // Verify transition types
    assert!(code.contains("pub struct SetCollection"));
    assert!(code.contains("pub struct SetRecord"));
    assert!(code.contains("pub struct SetRepo"));

    // Verify members module
    assert!(code.contains("pub mod members"));
    assert!(code.contains("pub struct collection"));
    assert!(code.contains("pub struct record"));
    assert!(code.contains("pub struct repo"));

    // Verify sealed trait
    assert!(code.contains("mod sealed"));

    // Verify it parses as valid Rust
    let _parsed: syn::File = syn::parse2(tokens).expect("Generated state module should parse");
}

#[test]
fn test_build_method_generation() {
    let fields = vec![RequiredField::new("repo"), RequiredField::new("collection")];

    // Create a simple object with required and optional fields
    let mut properties = BTreeMap::new();
    properties.insert(
        SmolStr::new_static("repo"),
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
    properties.insert(
        SmolStr::new_static("collection"),
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
    properties.insert(
        SmolStr::new_static("rkey"),
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

    let obj = LexObject {
        description: None,
        required: Some(vec![
            SmolStr::new_static("repo"),
            SmolStr::new_static("collection"),
        ]),
        nullable: None,
        properties,
    };

    let schema = BuilderSchema::Object(&obj);
    let tokens = build_method::generate_build_method("CreateRecord", &schema, &fields, true);
    let code = tokens.to_string();

    // Verify build method structure
    assert!(code.contains("pub fn build"));
    assert!(code.contains("CreateRecord"));
    assert!(code.contains("create_record_state"));
    assert!(code.contains("State"));

    // Verify where clauses for required fields (with flexible spacing)
    assert!(code.contains("Repo") && code.contains("IsSet"));
    assert!(code.contains("Collection") && code.contains("IsSet"));

    // Verify field extraction from tuple (fields ordered by BTreeMap key order)
    assert!(code.contains("collection : self . __unsafe_private_named . 0 . unwrap ()"));
    assert!(code.contains("repo : self . __unsafe_private_named . 1 . unwrap ()"));
    assert!(code.contains("rkey : self . __unsafe_private_named . 2")); // optional, no unwrap

    // Verify extra_data for LexObject
    assert!(code.contains("extra_data : Default :: default ()"));

    // Verify it parses as valid Rust
    let _parsed: syn::File = syn::parse2(tokens).expect("Generated build method should parse");
}

#[test]
fn test_build_method_parameters() {
    let fields = vec![RequiredField::new("limit")];

    let mut properties = BTreeMap::new();
    properties.insert(
        SmolStr::new_static("limit"),
        LexXrpcParametersProperty::Integer(LexInteger {
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            r#enum: None,
            r#const: None,
        }),
    );
    properties.insert(
        SmolStr::new_static("cursor"),
        LexXrpcParametersProperty::String(LexString {
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

    let params = LexXrpcParameters {
        description: None,
        required: Some(vec![SmolStr::new_static("limit")]),
        properties,
    };

    let schema = BuilderSchema::Parameters(&params);
    let tokens = build_method::generate_build_method("QueryParams", &schema, &fields, true);
    let code = tokens.to_string();

    // Verify build method structure
    assert!(code.contains("pub fn build"));
    assert!(code.contains("QueryParams"));

    // Verify NO extra_data for Parameters
    assert!(!code.contains("extra_data"));

    // Verify it parses as valid Rust
    let _parsed: syn::File = syn::parse2(tokens).expect("Generated build method should parse");
}

#[test]
fn test_complete_builder_object() {
    // Test that all components work together for a LexObject
    let mut properties = BTreeMap::new();
    properties.insert(
        SmolStr::new_static("repo"),
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
    properties.insert(
        SmolStr::new_static("collection"),
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
    properties.insert(
        SmolStr::new_static("rkey"),
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

    let obj = LexObject {
        description: None,
        required: Some(vec![
            SmolStr::new_static("repo"),
            SmolStr::new_static("collection"),
        ]),
        nullable: None,
        properties,
    };

    let schema = BuilderSchema::Object(&obj);
    let required_fields = collect_required_fields(&schema);

    // Generate all components
    let state_module = generate_state_module("CreateRecord", &required_fields);
    let build_method =
        build_method::generate_build_method("CreateRecord", &schema, &required_fields, true);

    let state_code = state_module.to_string();
    let build_code = build_method.to_string();

    // Verify state module
    assert!(state_code.contains("pub mod create_record_state"));
    assert!(state_code.contains("pub trait State"));
    assert!(state_code.contains("type Repo"));
    assert!(state_code.contains("type Collection"));
    assert!(state_code.contains("pub struct SetRepo"));
    assert!(state_code.contains("pub struct SetCollection"));

    // Verify build method
    assert!(build_code.contains("pub fn build"));
    assert!(build_code.contains("-> CreateRecord"));

    // Combine and verify parsing
    let combined = quote::quote! {
        #state_module
        #build_method
    };
    let _parsed: syn::File =
        syn::parse2(combined).expect("Complete builder components should parse");
}

#[test]
fn test_print_complete_builder() {
    // Generate and print a complete builder for inspection
    let mut properties = BTreeMap::new();
    properties.insert(
        SmolStr::new_static("repo"),
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
    properties.insert(
        SmolStr::new_static("collection"),
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
    properties.insert(
        SmolStr::new_static("rkey"),
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

    let obj = LexObject {
        description: None,
        required: Some(vec![
            SmolStr::new_static("repo"),
            SmolStr::new_static("collection"),
        ]),
        nullable: None,
        properties,
    };

    let schema = BuilderSchema::Object(&obj);
    let required_fields = collect_required_fields(&schema);

    // Generate all components
    let common_types = generate_common_types();
    let state_module = generate_state_module("CreateRecord", &required_fields);
    let build_method =
        build_method::generate_build_method("CreateRecord", &schema, &required_fields, true);

    // Note: Can't generate builder_struct and setters without CodeGenerator
    // But we can print what we have

    let combined = quote::quote! {
        #common_types
        #state_module
        #build_method
    };

    // Parse and format the code
    let parsed: syn::File = syn::parse2(combined).expect("Generated code should parse");
    let formatted = prettyplease::unparse(&parsed);

    println!("\n\n========== GENERATED BUILDER CODE ==========\n");
    println!("{}", formatted);
    println!("\n========== END GENERATED BUILDER CODE ==========\n\n");
}

#[test]
fn test_print_complete_builder_with_codegen() {
    // Create a minimal corpus and CodeGenerator for type conversion
    let corpus = LexiconCorpus::new();
    let codegen = CodeGenerator::new(&corpus, "test_crate");

    // Create a test object with required and optional fields
    let mut properties = BTreeMap::new();
    properties.insert(
        SmolStr::new_static("repo"),
        LexObjectProperty::String(LexString {
            description: None,
            format: Some(crate::lexicon::LexStringFormat::Did),
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
    properties.insert(
        SmolStr::new_static("collection"),
        LexObjectProperty::String(LexString {
            description: None,
            format: Some(crate::lexicon::LexStringFormat::Nsid),
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
    properties.insert(
        SmolStr::new_static("rkey"),
        LexObjectProperty::String(LexString {
            description: None,
            format: Some(crate::lexicon::LexStringFormat::RecordKey),
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
    properties.insert(
        SmolStr::new_static("validate"),
        LexObjectProperty::Boolean(crate::lexicon::LexBoolean {
            description: None,
            default: None,
            r#const: None,
        }),
    );

    let obj = LexObject {
        description: None,
        required: Some(vec![
            SmolStr::new_static("repo"),
            SmolStr::new_static("collection"),
        ]),
        nullable: None,
        properties,
    };

    // Use BuilderGenContext to generate complete builder
    let ctx = BuilderGenContext::from_object(
        &codegen,
        "com.atproto.repo.createRecord",
        "CreateRecord",
        &obj,
        true,
    );

    let builder_code = ctx.generate();

    // Also generate common types
    let common_types = generate_common_types();

    // Combine everything
    let combined = quote::quote! {
        #common_types
        #builder_code
    };

    // Parse and format the code
    let parsed: syn::File = syn::parse2(combined).expect("Generated code should parse");
    let formatted = prettyplease::unparse(&parsed);

    println!("\n\n========== COMPLETE BUILDER WITH STRUCT AND SETTERS ==========\n");
    println!("{}", formatted);
    println!("\n========== END COMPLETE BUILDER ==========\n\n");

    // Verify key components are present
    let code = formatted;
    assert!(code.contains("pub struct CreateRecordBuilder"));
    assert!(code.contains("pub mod create_record_state"));
    assert!(code.contains("pub fn repo("));
    assert!(code.contains("pub fn collection("));
    assert!(code.contains("pub fn rkey("));
    assert!(code.contains("pub fn maybe_rkey("));
    assert!(code.contains("pub fn validate("));
    assert!(code.contains("pub fn build(self)"));
    assert!(code.contains("-> CreateRecord"));
}
