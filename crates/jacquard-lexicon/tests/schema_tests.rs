use jacquard_common::CowStr;
use jacquard_common::types::string::Datetime;
use jacquard_lexicon::lexicon::{
    LexObject, LexObjectProperty, LexRecord, LexRecordRecord, LexString, LexStringFormat,
    LexUserType, Lexicon, LexiconDoc,
};
use jacquard_lexicon::schema::LexiconSchema;
use jacquard_lexicon::validation::{ConstraintError, ValidationPath};
use std::collections::BTreeMap;

// Simple test type
#[derive(Debug, Clone)]
struct SimpleRecord<'a> {
    text: CowStr<'a>,
    #[allow(dead_code)]
    timestamp: Datetime,
}

impl LexiconSchema for SimpleRecord<'_> {
    fn nsid() -> &'static str {
        "com.example.simple"
    }

    fn lexicon_doc() -> LexiconDoc<'static> {
        let mut properties = BTreeMap::new();

        properties.insert(
            "text".into(),
            LexObjectProperty::String(LexString {
                description: None,
                format: None,
                default: None,
                min_length: None,
                max_length: Some(1000),
                min_graphemes: None,
                max_graphemes: None,
                r#enum: None,
                r#const: None,
                known_values: None,
            }),
        );

        properties.insert(
            "timestamp".into(),
            LexObjectProperty::String(LexString {
                description: None,
                format: Some(LexStringFormat::Datetime),
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

        let record_obj = LexObject {
            description: None,
            required: Some(vec!["text".into(), "timestamp".into()]),
            nullable: None,
            properties,
        };

        let record = LexRecord {
            description: Some("Simple record type".into()),
            key: Some("tid".into()),
            record: LexRecordRecord::Object(record_obj),
        };

        let mut defs = BTreeMap::new();
        defs.insert("main".into(), LexUserType::Record(record));

        LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: Self::nsid().into(),
            revision: None,
            description: Some("Test schema".into()),
            defs,
        }
    }

    fn validate(&self) -> Result<(), ConstraintError> {
        // Check text length
        if self.text.len() > 1000 {
            return Err(ConstraintError::MaxLength {
                path: ValidationPath::from_field("text"),
                max: 1000,
                actual: self.text.len(),
            });
        }

        Ok(())
    }
}

#[test]
fn test_manual_impl_generates_valid_schema() {
    let doc = SimpleRecord::lexicon_doc();

    // Verify structure
    assert_eq!(doc.id.as_ref(), "com.example.simple");
    assert!(doc.defs.contains_key("main"));

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&doc).expect("serialize");
    println!("{}", json);

    // Should be valid lexicon JSON
    assert!(json.contains("\"lexicon\": 1"));
    assert!(json.contains("\"id\": \"com.example.simple\""));
}

#[test]
fn test_validation_works() {
    let record = SimpleRecord {
        text: "a".repeat(5000).into(), // Too long
        timestamp: Datetime::now(),
    };

    let result = record.validate();
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(err, ConstraintError::MaxLength { .. }));
}

#[test]
fn test_validation_passes() {
    let record = SimpleRecord {
        text: "Hello, world!".into(),
        timestamp: Datetime::now(),
    };

    let result = record.validate();
    assert!(result.is_ok());
}
