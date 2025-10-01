use jacquard_derive::open_union;
use serde::{Deserialize, Serialize};

#[open_union]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "$type")]
enum TestUnion<'s> {
    #[serde(rename = "com.example.typeA")]
    TypeA { value: &'s str },
    #[serde(rename = "com.example.typeB")]
    TypeB { count: i64 },
}

#[test]
fn test_open_union_known_variant() {
    let json = r#"{"$type":"com.example.typeA","value":"hello"}"#;
    let union: TestUnion = serde_json::from_str(json).unwrap();

    match union {
        TestUnion::TypeA { value } => assert_eq!(value, "hello"),
        _ => panic!("expected TypeA"),
    }
}

#[test]
fn test_open_union_unknown_variant() {
    use jacquard_common::types::value::{Data, Object};

    let json = r#"{"$type":"com.example.unknown","data":"something"}"#;
    let union: TestUnion = serde_json::from_str(json).unwrap();

    match union {
        TestUnion::Unknown(Data::Object(obj)) => {
            // Verify the captured data contains the expected fields
            assert!(obj.0.contains_key("$type"));
            assert!(obj.0.contains_key("data"));

            // Check the actual values
            if let Some(Data::String(type_str)) = obj.0.get("$type") {
                assert_eq!(type_str.as_str(), "com.example.unknown");
            } else {
                panic!("expected $type field to be a string");
            }

            if let Some(Data::String(data_str)) = obj.0.get("data") {
                assert_eq!(data_str.as_str(), "something");
            } else {
                panic!("expected data field to be a string");
            }
        }
        _ => panic!("expected Unknown variant with Object data"),
    }
}

#[test]
fn test_open_union_roundtrip() {
    let union = TestUnion::TypeB { count: 42 };
    let json = serde_json::to_string(&union).unwrap();
    let parsed: TestUnion = serde_json::from_str(&json).unwrap();

    assert_eq!(union, parsed);

    // Verify the $type field is present
    assert!(json.contains(r#""$type":"com.example.typeB""#));
}

#[test]
fn test_open_union_unknown_roundtrip() {
    use jacquard_common::types::value::{Data, Object};
    use std::collections::BTreeMap;

    // Create an Unknown variant with complex data
    let mut map = BTreeMap::new();
    map.insert(
        "$type".into(),
        Data::String(jacquard_common::types::string::AtprotoStr::String(
            "com.example.custom".into(),
        )),
    );
    map.insert("field1".into(), Data::Integer(123));
    map.insert("field2".into(), Data::Boolean(false));

    let union = TestUnion::Unknown(Data::Object(Object(map)));

    let json = serde_json::to_string(&union).unwrap();
    let parsed: TestUnion = serde_json::from_str(&json).unwrap();

    // Should deserialize back as Unknown since the type is not recognized
    match parsed {
        TestUnion::Unknown(Data::Object(obj)) => {
            assert_eq!(obj.0.len(), 3);
            assert!(obj.0.contains_key("$type"));
            assert!(obj.0.contains_key("field1"));
            assert!(obj.0.contains_key("field2"));

            // Verify values
            if let Some(Data::String(s)) = obj.0.get("$type") {
                assert_eq!(s.as_str(), "com.example.custom");
            } else {
                panic!("expected $type to be a string");
            }

            if let Some(Data::Integer(n)) = obj.0.get("field1") {
                assert_eq!(*n, 123);
            } else {
                panic!("expected field1 to be an integer");
            }

            if let Some(Data::Boolean(b)) = obj.0.get("field2") {
                assert_eq!(*b, false);
            } else {
                panic!("expected field2 to be a boolean");
            }
        }
        _ => panic!("expected Unknown variant"),
    }
}
