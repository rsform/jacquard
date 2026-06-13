use jacquard_common::CowStr;
use jacquard_common::types::string::Datetime;
use jacquard_derive::{LexiconSchema, open_union};
use jacquard_lexicon::schema::LexiconSchema as LexiconSchemaTrait;
use serde::{Deserialize, Serialize};
extern crate alloc;

#[test]
fn test_simple_struct() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.simple", record, key = "tid")]
    struct SimpleRecord<'a> {
        #[allow(dead_code)]
        pub text: CowStr<'a>,
        #[allow(dead_code)]
        pub created_at: Datetime,
    }

    assert_eq!(SimpleRecord::nsid(), "com.example.simple");
    assert_eq!(SimpleRecord::schema_id().as_ref(), "com.example.simple");

    let doc = SimpleRecord::lexicon_doc();

    assert_eq!(doc.id.as_ref(), "com.example.simple");
    assert!(doc.defs.contains_key("main"));

    // Serialize to JSON to verify structure
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should contain record type
    assert!(json.contains("\"type\": \"record\""));
    // Should have camelCase field names (default)
    assert!(json.contains("\"createdAt\""));
}

#[test]
fn test_struct_with_constraints() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.constrained", record)]
    struct ConstrainedRecord<'a> {
        #[lexicon(max_graphemes = 300, max_length = 3000)]
        pub text: CowStr<'a>,

        #[lexicon(minimum = 0, maximum = 100)]
        pub score: i64,
    }

    let doc = ConstrainedRecord::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Verify constraints are in schema
    assert!(json.contains("\"maxGraphemes\": 300"));
    assert!(json.contains("\"maxLength\": 3000"));
    assert!(json.contains("\"minimum\": 0"));
    assert!(json.contains("\"maximum\": 100"));
}

#[test]
fn test_validation() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.validated", record)]
    struct ValidatedRecord<'a> {
        #[lexicon(max_length = 100)]
        pub text: CowStr<'a>,

        #[lexicon(minimum = 0, maximum = 10)]
        pub count: i64,
    }

    // Valid
    let valid = ValidatedRecord {
        text: "hello".into(),
        count: 5,
    };
    assert!(valid.validate().is_ok());

    // Text too long
    let invalid_text = ValidatedRecord {
        text: "a".repeat(150).into(),
        count: 5,
    };
    assert!(invalid_text.validate().is_err());

    // Count too high
    let invalid_count = ValidatedRecord {
        text: "hello".into(),
        count: 15,
    };
    assert!(invalid_count.validate().is_err());

    // Count too low
    let invalid_low = ValidatedRecord {
        text: "hello".into(),
        count: -5,
    };
    assert!(invalid_low.validate().is_err());
}

#[test]
fn test_serde_rename() {
    #[derive(Serialize, Deserialize, LexiconSchema)]
    #[lexicon(nsid = "com.example.renamed", record)]
    #[serde(rename_all = "snake_case")]
    struct RenamedRecord {
        pub some_field: i64,
        pub another_field: i64,
    }
    let doc = RenamedRecord::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should use snake_case not camelCase
    assert!(json.contains("\"some_field\""));
    assert!(json.contains("\"another_field\""));
}

#[test]
fn test_default_camel_case() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.camel", record)]
    struct CamelCaseRecord {
        #[allow(dead_code)]
        pub field_one: i64,
        #[allow(dead_code)]
        pub field_two: i64,
    }

    let doc = CamelCaseRecord::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should default to camelCase
    assert!(json.contains("\"fieldOne\""));
    assert!(json.contains("\"fieldTwo\""));
}

#[test]
fn test_basic_enum() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.union")]
    enum BasicUnion {
        #[nsid = "com.example.variant.one"]
        #[allow(dead_code)]
        VariantOne,

        #[nsid = "com.example.variant.two"]
        #[allow(dead_code)]
        VariantTwo,
    }

    let doc = BasicUnion::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should be a union type
    assert!(json.contains("\"type\": \"union\""));
    // Should have refs
    assert!(json.contains("com.example.variant.one"));
    assert!(json.contains("com.example.variant.two"));
    // Should be closed by default
    assert!(json.contains("\"closed\": true"));
}

#[test]
fn test_open_union_attribute_adds_unknown_variant() {
    #[open_union]
    #[derive(Serialize, Deserialize, LexiconSchema)]
    #[lexicon(nsid = "com.example.open")]
    enum OpenUnion<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[nsid = "com.example.variant"]
        #[allow(dead_code)]
        Variant,
    }

    let doc = OpenUnion::<jacquard_common::DefaultStr>::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should be a union type with known refs, while remaining open by omitting closed.
    assert!(json.contains("\"type\": \"union\""));
    assert!(json.contains("com.example.variant"));
    assert!(!json.contains("\"closed\""));
}

#[test]
fn test_open_union_detects_existing_unknown_variant() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.open_existing")]
    enum OpenUnion<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[nsid = "com.example.variant"]
        #[allow(dead_code)]
        Variant,

        #[allow(dead_code)]
        Unknown(jacquard_common::types::value::Data<S>),
    }

    let doc = OpenUnion::<jacquard_common::DefaultStr>::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"union\""));
    assert!(json.contains("com.example.variant"));
    assert!(!json.contains("\"closed\""));
}

#[test]
fn test_generic_record() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.generic", record)]
    struct GenericRecord<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[lexicon(max_length = 100)]
        #[allow(dead_code)]
        text: S,
    }

    let doc = GenericRecord::<jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"string\""));
    assert!(json.contains("\"maxLength\": 100"));

    let record = GenericRecord {
        text: jacquard_common::DefaultStr::from("hello"),
    };
    assert!(record.validate().is_ok());
}

#[test]
fn test_optional_generic_record() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.optional_generic", record)]
    struct OptionalGeneric<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[lexicon(max_length = 100)]
        #[allow(dead_code)]
        text: Option<S>,
    }

    let doc = OptionalGeneric::<jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"string\""));
    assert!(json.contains("\"maxLength\": 100"));
    assert!(!json.contains("\"required\""));
}

#[test]
fn test_generic_array_record() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.generic_array", record)]
    struct GenericArray<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[lexicon(max_items = 5)]
        #[allow(dead_code)]
        tags: Vec<S>,
    }

    let doc = GenericArray::<jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"array\""));
    assert!(json.contains("\"items\""));
    assert!(json.contains("\"type\": \"string\""));
    assert!(json.contains("\"maxLength\": 5"));
}

#[test]
fn test_non_s_bos_type_parameter_maps_to_string() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.non_s_generic", record)]
    struct GenericRecord<Text: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[allow(dead_code)]
        text: Text,
    }

    let doc = GenericRecord::<jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"string\""));
}

#[test]
fn test_old_style_bos_bound_maps_to_string() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.old_style_bos", record)]
    struct GenericRecord<S: jacquard_common::Bos<str> + AsRef<str> = jacquard_common::DefaultStr> {
        #[allow(dead_code)]
        text: S,
    }

    let doc = GenericRecord::<jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"string\""));
}

#[test]
fn test_lifetime_and_bos_type_parameter_record() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.mixed_generics", record)]
    struct Mixed<'a, S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[allow(dead_code)]
        borrowed: CowStr<'a>,
        #[allow(dead_code)]
        text: S,
    }

    let doc = Mixed::<'static, jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert_eq!(json.matches("\"type\": \"string\"").count(), 2);
}

#[test]
fn test_generic_open_union_schema_and_unknown_roundtrip() {
    use jacquard_common::types::value::{Data, Object};
    use std::collections::BTreeMap;

    #[open_union]
    #[derive(Serialize, Deserialize, Debug, PartialEq, LexiconSchema)]
    #[serde(tag = "$type")]
    #[lexicon(nsid = "com.example.generic_open_roundtrip")]
    enum GenericOpenUnion<S: jacquard_common::BosStr = jacquard_common::DefaultStr> {
        #[serde(rename = "com.example.known")]
        #[nsid = "com.example.known"]
        Known { value: S },
    }

    let doc = GenericOpenUnion::<jacquard_common::DefaultStr>::lexicon_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    assert!(json.contains("\"type\": \"union\""));
    assert!(json.contains("com.example.known"));
    assert!(!json.contains("\"closed\""));

    let unknown_json = r#"{"$type":"com.example.unknown","value":"hello"}"#;
    let parsed: GenericOpenUnion = serde_json::from_str(unknown_json).unwrap();
    match parsed {
        GenericOpenUnion::Unknown(Data::Object(obj)) => {
            assert!(obj.0.contains_key("$type"));
            assert!(obj.0.contains_key("value"));
        }
        _ => panic!("expected Unknown variant"),
    }

    let mut map = BTreeMap::new();
    map.insert(
        "$type".into(),
        Data::String(jacquard_common::types::string::AtprotoStr::String(
            jacquard_common::DefaultStr::from("com.example.other"),
        )),
    );
    map.insert("count".into(), Data::Integer(7));

    let union = GenericOpenUnion::Unknown(Data::Object(Object(map)));
    let serialized = serde_json::to_string(&union).unwrap();
    let roundtripped: GenericOpenUnion = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(
        roundtripped,
        GenericOpenUnion::Unknown(Data::Object(_))
    ));
}

#[test]
fn test_enum_with_serde_rename() {
    #[derive(Serialize, Deserialize, LexiconSchema)]
    #[lexicon(nsid = "com.example.renamed_union")]
    enum RenamedUnion {
        #[serde(rename = "app.bsky.embed.images")]
        Images,

        #[serde(rename = "app.bsky.embed.video")]
        Video,
    }

    let doc = RenamedUnion::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should use serde rename values
    assert!(json.contains("app.bsky.embed.images"));
    assert!(json.contains("app.bsky.embed.video"));
}

#[test]
fn test_enum_fragment_inference() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.fragments")]
    enum FragmentUnion {
        // Should generate com.example.fragments#variantOne
        #[allow(dead_code)]
        VariantOne,
        // Should generate com.example.fragments#variantTwo
        #[allow(dead_code)]
        VariantTwo,
    }
    let doc = FragmentUnion::lexicon_doc();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should have fragment refs
    assert!(json.contains("com.example.fragments#variantOne"));
    assert!(json.contains("com.example.fragments#variantTwo"));
}
