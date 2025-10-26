use jacquard_common::CowStr;
use jacquard_common::types::string::Datetime;
use jacquard_derive::{LexiconSchema, open_union};
use jacquard_lexicon::schema::{LexiconGenerator, LexiconSchema as LexiconSchemaTrait};
use serde::{Deserialize, Serialize};

#[test]
fn test_simple_struct() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.simple", record, key = "tid")]
    struct SimpleRecord<'a> {
        pub text: CowStr<'a>,
        pub created_at: Datetime,
    }

    assert_eq!(SimpleRecord::nsid(), "com.example.simple");
    assert_eq!(SimpleRecord::schema_id().as_ref(), "com.example.simple");

    let mut generator = LexiconGenerator::new(SimpleRecord::nsid());
    let doc = SimpleRecord::lexicon_doc(&mut generator);

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

    let mut generator = LexiconGenerator::new(ConstrainedRecord::nsid());
    let doc = ConstrainedRecord::lexicon_doc(&mut generator);

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

    let mut generator = LexiconGenerator::new(RenamedRecord::nsid());
    let doc = RenamedRecord::lexicon_doc(&mut generator);

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
        pub field_one: i64,
        pub field_two: i64,
    }

    let mut generator = LexiconGenerator::new(CamelCaseRecord::nsid());
    let doc = CamelCaseRecord::lexicon_doc(&mut generator);

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
        VariantOne,

        #[nsid = "com.example.variant.two"]
        VariantTwo,
    }

    let mut generator = LexiconGenerator::new(BasicUnion::nsid());
    let doc = BasicUnion::lexicon_doc(&mut generator);

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
fn test_open_union() {
    #[derive(LexiconSchema)]
    #[lexicon(nsid = "com.example.open")]
    #[open_union]
    enum OpenUnion<'a> {
        #[nsid = "com.example.variant"]
        Variant,

        Unknown(jacquard_common::types::value::Data<'a>),
    }

    let mut generator = LexiconGenerator::new(OpenUnion::nsid());
    let doc = OpenUnion::lexicon_doc(&mut generator);

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should be open (closed field omitted, defaults to open)
    assert!(!json.contains("\"closed\""));
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

    let mut generator = LexiconGenerator::new(RenamedUnion::nsid());
    let doc = RenamedUnion::lexicon_doc(&mut generator);

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
        VariantOne,
        // Should generate com.example.fragments#variantTwo
        VariantTwo,
    }

    let mut generator = LexiconGenerator::new(FragmentUnion::nsid());
    let doc = FragmentUnion::lexicon_doc(&mut generator);

    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("{}", json);

    // Should have fragment refs
    assert!(json.contains("com.example.fragments#variantOne"));
    assert!(json.contains("com.example.fragments#variantTwo"));
}
