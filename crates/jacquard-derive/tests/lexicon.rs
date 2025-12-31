use jacquard_derive::lexicon;
use serde::{Deserialize, Serialize};
extern crate alloc;

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

    let extra_data = record.extra_data.unwrap();
    assert_eq!(extra_data.len(), 2);
    assert!(extra_data.contains_key("unknown"));
    assert!(extra_data.contains_key("another"));
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
    extra.insert("number".into(), Data::Integer(42));
    extra.insert(
        "nested".into(),
        Data::Object(jacquard_common::types::value::Object({
            let mut nested_map = BTreeMap::new();
            nested_map.insert("inner".into(), Data::Boolean(true));
            nested_map
        })),
    );

    let record = TestRecord {
        text: "test",
        count: 100,
        extra_data: Some(extra),
    };

    let json = serde_json::to_string(&record).unwrap();
    let parsed: TestRecord = serde_json::from_str(&json).unwrap();

    assert_eq!(record, parsed);
    let extra_data = parsed.extra_data.unwrap();
    assert_eq!(extra_data.len(), 3);

    // Verify the extra fields were preserved
    assert!(extra_data.contains_key("custom"));
    assert!(extra_data.contains_key("number"));
    assert!(extra_data.contains_key("nested"));

    // Verify the values
    if let Some(Data::String(s)) = extra_data.get("custom") {
        assert_eq!(s.as_str(), "value");
    } else {
        panic!("expected custom field to be a string");
    }

    if let Some(Data::Integer(n)) = extra_data.get("number") {
        assert_eq!(*n, 42);
    } else {
        panic!("expected number field to be an integer");
    }

    if let Some(Data::Object(obj)) = extra_data.get("nested") {
        assert!(obj.0.contains_key("inner"));
    } else {
        panic!("expected nested field to be an object");
    }
}
