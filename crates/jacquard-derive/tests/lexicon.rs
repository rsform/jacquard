use jacquard_derive::lexicon;
use serde::{Deserialize, Serialize};

#[lexicon]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct TestRecord<'s> {
    text: &'s str,
    count: i64,
}

#[test]
fn test_lexicon_adds_extra_data_field() {
    let json = r#"{"text":"hello","count":42,"unknown":"field","another":123}"#;

    let record: TestRecord = serde_json::from_str(json).unwrap();

    assert_eq!(record.text, "hello");
    assert_eq!(record.count, 42);
    assert_eq!(record.extra_data.len(), 2);
    assert!(record.extra_data.contains_key("unknown"));
    assert!(record.extra_data.contains_key("another"));
}

#[test]
fn test_lexicon_roundtrip() {
    use jacquard_common::CowStr;
    use jacquard_common::types::value::Data;
    use std::collections::BTreeMap;

    let mut extra = BTreeMap::new();
    extra.insert(
        "custom".into(),
        Data::String(jacquard_common::types::string::AtprotoStr::String(
            CowStr::Borrowed("value"),
        )),
    );
    extra.insert(
        "number".into(),
        Data::Integer(42),
    );
    extra.insert(
        "nested".into(),
        Data::Object(jacquard_common::types::value::Object({
            let mut nested_map = BTreeMap::new();
            nested_map.insert(
                "inner".into(),
                Data::Boolean(true),
            );
            nested_map
        })),
    );

    let record = TestRecord {
        text: "test",
        count: 100,
        extra_data: extra,
    };

    let json = serde_json::to_string(&record).unwrap();
    let parsed: TestRecord = serde_json::from_str(&json).unwrap();

    assert_eq!(record, parsed);
    assert_eq!(parsed.extra_data.len(), 3);

    // Verify the extra fields were preserved
    assert!(parsed.extra_data.contains_key("custom"));
    assert!(parsed.extra_data.contains_key("number"));
    assert!(parsed.extra_data.contains_key("nested"));

    // Verify the values
    if let Some(Data::String(s)) = parsed.extra_data.get("custom") {
        assert_eq!(s.as_str(), "value");
    } else {
        panic!("expected custom field to be a string");
    }

    if let Some(Data::Integer(n)) = parsed.extra_data.get("number") {
        assert_eq!(*n, 42);
    } else {
        panic!("expected number field to be an integer");
    }

    if let Some(Data::Object(obj)) = parsed.extra_data.get("nested") {
        assert!(obj.0.contains_key("inner"));
    } else {
        panic!("expected nested field to be an object");
    }
}
