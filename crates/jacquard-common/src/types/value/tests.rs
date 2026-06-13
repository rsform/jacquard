use crate::cowstr::ToCowStr;

use super::*;
use core::str::FromStr;
use std::string::{String, ToString};

/// Canonicalize JSON by sorting object keys recursively
fn canonicalize_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted_map = serde_json::Map::new();
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted_map.insert(key.clone(), canonicalize_json(&map[key]));
            }
            serde_json::Value::Object(sorted_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn serialize_deserialize_null() {
    let data = Data::Null;

    // JSON roundtrip
    let json = serde_json::to_string(&data).unwrap();
    assert_eq!(json, "null");
    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);
    assert!(matches!(parsed, Data::Null));
}

#[test]
fn serialize_deserialize_boolean() {
    let data = Data::Boolean(true);

    let json = serde_json::to_string(&data).unwrap();
    assert_eq!(json, "true");
    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);
}

#[test]
fn serialize_deserialize_integer() {
    let data = Data::Integer(42);

    let json = serde_json::to_string(&data).unwrap();
    assert_eq!(json, "42");
    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);
}

#[test]
fn serialize_deserialize_string() {
    let data = Data::String(AtprotoStr::String("hello world".into()));

    let json = serde_json::to_string(&data).unwrap();
    assert_eq!(json, r#""hello world""#);
    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);
}

#[test]
fn serialize_deserialize_bytes_json() {
    let data = Data::Bytes(Bytes::from_static(b"hello"));

    // JSON: should be {"$bytes": "base64"}
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("$bytes"));
    assert!(json.contains("aGVsbG8=")); // base64("hello")

    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);
}

#[test]
fn serialize_deserialize_cid_link_json() {
    let data = Data::CidLink(Cid::cow_str(CowStr::Borrowed(
        "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha",
    )));

    // JSON: should be {"$link": "cid_string"}
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("$link"));
    assert!(json.contains("bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"));

    let parsed: Data = serde_json::from_str(&json).unwrap();
    match parsed {
        Data::CidLink(cid) => assert_eq!(
            cid.as_str(),
            "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"
        ),
        _ => panic!("expected CidLink"),
    }
}

#[test]
fn serialize_deserialize_array() {
    let data = Data::Array(Array(vec![
        Data::Null,
        Data::Boolean(true),
        Data::Integer(42),
        Data::String(AtprotoStr::String("test".into())),
    ]));

    let json = serde_json::to_string(&data).unwrap();
    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);

    // Verify structure
    if let Data::Array(Array(items)) = parsed {
        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], Data::Null));
        assert!(matches!(items[1], Data::Boolean(true)));
        assert!(matches!(items[2], Data::Integer(42)));
        if let Data::String(AtprotoStr::String(s)) = &items[3] {
            assert_eq!(s, "test");
        } else {
            panic!("expected plain string");
        }
    } else {
        panic!("expected array");
    }
}

#[test]
fn serialize_deserialize_object() {
    let mut map = BTreeMap::new();
    map.insert(
        "name".to_smolstr(),
        Data::String(AtprotoStr::String("alice".into())),
    );
    map.insert("age".to_smolstr(), Data::Integer(30));
    map.insert("active".to_smolstr(), Data::Boolean(true));

    let data = Data::Object(Object(map));

    let json = serde_json::to_string(&data).unwrap();
    let parsed: Data = serde_json::from_str(&json).unwrap();
    assert_eq!(data, parsed);
}

#[test]
fn type_inference_datetime() {
    // Field name "createdAt" should infer datetime type
    let json = r#"{"createdAt": "2023-01-15T12:30:45.123456Z"}"#;
    let data: Data = serde_json::from_str(json).unwrap();

    if let Data::Object(obj) = data {
        if let Some(Data::String(AtprotoStr::Datetime(dt))) = obj.0.get("createdAt") {
            // Verify it's actually parsed correctly
            assert_eq!(dt.as_str(), "2023-01-15T12:30:45.123456Z");
        } else {
            panic!("createdAt should be parsed as Datetime");
        }
    } else {
        panic!("expected object");
    }
}

#[test]
fn type_inference_did() {
    let json = r#"{"did": "did:plc:abc123"}"#;
    let data: Data = serde_json::from_str(json).unwrap();

    if let Data::Object(obj) = data {
        if let Some(Data::String(AtprotoStr::Did(did))) = obj.0.get("did") {
            assert_eq!(did.as_str(), "did:plc:abc123");
        } else {
            panic!("did should be parsed as Did");
        }
    } else {
        panic!("expected object");
    }
}

#[test]
fn type_inference_uri() {
    let json = r#"{"uri": "at://alice.test/com.example.foo/123"}"#;
    let data: Data = serde_json::from_str(json).unwrap();

    if let Data::Object(obj) = data {
        // "uri" field gets inferred as Uri type, but at:// should parse to AtUri
        match obj.0.get("uri") {
            Some(Data::String(AtprotoStr::AtUri(_))) | Some(Data::String(AtprotoStr::Uri(_))) => {
                // Success
            }
            _ => panic!("uri should be parsed as Uri or AtUri"),
        }
    } else {
        panic!("expected object");
    }
}

#[test]
fn blob_deserialization() {
    let json = r#"{
        "$type": "blob",
        "ref": {"$link": "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"},
        "mimeType": "image/png",
        "size": 12345
    }"#;

    let data: Data = serde_json::from_str(json).unwrap();

    if let Data::Blob(blob) = data {
        assert_eq!(blob.mime_type.as_str(), "image/png");
        assert_eq!(blob.size, 12345);
    } else {
        panic!("expected blob");
    }
}

#[test]
fn data_borrowed_plain_string_deserialization() {
    let data: Data<&str> = serde_json::from_str(r#""hello""#).unwrap();

    match data {
        Data::String(AtprotoStr::String(s)) => assert_eq!(s, "hello"),
        other => panic!("expected borrowed plain string, got {other:?}"),
    }
}

#[test]
fn data_borrowed_nested_object_string_deserialization() {
    let data: Data<&str> = serde_json::from_str(r#"{"label":"plain nested value"}"#).unwrap();

    let Data::Object(obj) = data else {
        panic!("expected object");
    };

    match obj.0.get("label") {
        Some(Data::String(AtprotoStr::String(s))) => assert_eq!(*s, "plain nested value"),
        other => panic!("expected nested borrowed plain string, got {other:?}"),
    }
}

#[test]
fn data_integer_bounds_and_float_preservation() {
    let max: Data = serde_json::from_str("9223372036854775807").unwrap();
    assert_eq!(max, Data::Integer(i64::MAX));

    assert!(serde_json::from_str::<Data>("9223372036854775808").is_err());
    assert!(serde_json::from_str::<Data>("18446744073709551615").is_err());

    let float: Data = serde_json::from_str("42.5").unwrap();
    assert!(matches!(float, Data::InvalidNumber(_)));
}

#[test]
fn raw_data_preserves_unsigned_integer_bounds() {
    let raw: RawData = serde_json::from_str("18446744073709551615").unwrap();
    assert!(matches!(raw, RawData::UnsignedInt(u64::MAX)));
}

#[test]
fn raw_data_to_data_rejects_unsigned_integer_overflow() {
    let result = Data::<crate::DefaultStr>::try_from(RawData::UnsignedInt(u64::MAX));
    assert!(result.is_err());
}

#[test]
fn raw_data_to_data_rejects_invalid_blob_sizes() {
    let mut negative_size = BTreeMap::new();
    negative_size.insert(SmolStr::new_static("$type"), RawData::String("blob".into()));
    negative_size.insert(
        SmolStr::new_static("ref"),
        RawData::CidLink(Cid::cow_str(CowStr::Borrowed(
            "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha",
        ))),
    );
    negative_size.insert(
        SmolStr::new_static("mimeType"),
        RawData::String("image/png".into()),
    );
    negative_size.insert(SmolStr::new_static("size"), RawData::SignedInt(-1));

    let result = Data::<crate::DefaultStr>::try_from(RawData::Object(negative_size));
    assert!(result.is_err());

    let mut huge_size = BTreeMap::new();
    huge_size.insert(SmolStr::new_static("$type"), RawData::String("blob".into()));
    huge_size.insert(
        SmolStr::new_static("ref"),
        RawData::CidLink(Cid::cow_str(CowStr::Borrowed(
            "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha",
        ))),
    );
    huge_size.insert(
        SmolStr::new_static("mimeType"),
        RawData::String("image/png".into()),
    );
    huge_size.insert(SmolStr::new_static("size"), RawData::UnsignedInt(u64::MAX));

    if usize::try_from(u64::MAX).is_err() {
        let result = Data::<crate::DefaultStr>::try_from(RawData::Object(huge_size));
        assert!(result.is_err());
    }
}

#[test]
fn nested_objects() {
    let json = r#"{
        "user": {
            "name": "alice",
            "profile": {
                "bio": "test bio",
                "createdAt": "2023-01-15T12:30:45Z"
            }
        }
    }"#;

    let data: Data = serde_json::from_str(json).unwrap();

    // Should successfully parse with nested type inference
    if let Data::Object(obj) = data {
        assert!(obj.0.contains_key("user"));
    } else {
        panic!("expected object");
    }
}

#[test]
fn integration_bluesky_thread() {
    // Real bluesky thread data with complex nested structures
    let json = include_str!("test_thread.json");
    let data: Data = serde_json::from_str(json).unwrap();

    // Verify top-level structure
    if let Data::Object(obj) = data {
        // Should have "thread" array
        assert!(obj.0.contains_key("thread"));

        // Verify thread is an array
        if let Some(Data::Array(thread)) = obj.0.get("thread") {
            assert!(!thread.0.is_empty());

            // Check first thread item
            if let Some(Data::Object(item)) = thread.0.first() {
                // Should have "uri" field parsed as AtUri
                if let Some(Data::String(AtprotoStr::AtUri(uri))) = item.0.get("uri") {
                    assert!(uri.as_str().starts_with("at://did:plc:"));
                }

                // Should have "value" object
                if let Some(Data::Object(value)) = item.0.get("value") {
                    // Should have post object
                    if let Some(Data::Object(post)) = value.0.get("post") {
                        // CID should be parsed as Cid
                        if let Some(Data::String(AtprotoStr::Cid(cid))) = post.0.get("cid") {
                            assert!(cid.as_str().starts_with("bafy"));
                        }

                        // Author should have DID
                        if let Some(Data::Object(author)) = post.0.get("author") {
                            if let Some(Data::String(AtprotoStr::Did(did))) = author.0.get("did") {
                                assert!(did.as_str().starts_with("did:plc:"));
                            }

                            // createdAt should be parsed as Datetime
                            if let Some(Data::String(AtprotoStr::Datetime(_))) =
                                author.0.get("createdAt")
                            {
                                // Success
                            } else {
                                panic!("author.createdAt should be Datetime");
                            }
                        }
                    }
                }
            }
        } else {
            panic!("thread should be an array");
        }

        // Verify serialization produces same JSON structure
        let serialized = serde_json::to_string(&obj).unwrap();

        // Parse both as generic serde_json::Value to compare structure
        let original_value: serde_json::Value = serde_json::from_str(json).unwrap();
        let serialized_value: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Canonicalize by sorting keys
        let original_canonical = canonicalize_json(&original_value);
        let serialized_canonical = canonicalize_json(&serialized_value);

        assert_eq!(
            original_canonical, serialized_canonical,
            "Serialized JSON should match original structure"
        )
    } else {
        panic!("expected top-level object");
    }
}

#[test]
fn test_from_data_struct() {
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct TestStruct<'a> {
        #[serde(borrow)]
        name: &'a str,
        age: i64,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("name"),
        Data::String(AtprotoStr::String("Alice")),
    );
    map.insert(SmolStr::new_static("age"), Data::Integer(30));
    let data = Data::Object(Object(map));

    let result: TestStruct = from_data(&data).unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.age, 30);
}

#[test]
fn test_from_data_vec() {
    let data: Data = Data::Array(Array(vec![
        Data::Integer(1),
        Data::Integer(2),
        Data::Integer(3),
    ]));

    let result: Vec<i64> = from_data(&data).unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_from_data_nested() {
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct Nested<'a> {
        #[serde(borrow)]
        value: &'a str,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    struct Parent<'a> {
        #[serde(borrow)]
        nested: Nested<'a>,
        count: i64,
    }

    let mut nested_map = BTreeMap::new();
    nested_map.insert(
        SmolStr::new_static("value"),
        Data::String(AtprotoStr::String("test")),
    );

    let mut parent_map = BTreeMap::new();
    parent_map.insert(
        SmolStr::new_static("nested"),
        Data::Object(Object(nested_map)),
    );
    parent_map.insert(SmolStr::new_static("count"), Data::Integer(42));

    let data = Data::Object(Object(parent_map));

    let result: Parent = from_data(&data).unwrap();
    assert_eq!(result.nested.value, "test");
    assert_eq!(result.count, 42);
}

#[test]
fn test_from_raw_data_struct() {
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct TestStruct<'a> {
        #[serde(borrow)]
        name: &'a str,
        age: u64,
    }

    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("name"), RawData::String("Bob".into()));
    map.insert(SmolStr::new_static("age"), RawData::UnsignedInt(25));
    let data = RawData::Object(map);

    let result: TestStruct = from_raw_data(&data).unwrap();
    assert_eq!(result.name, "Bob");
    assert_eq!(result.age, 25);
}

#[test]
fn test_from_data_option() {
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct WithOption<'a> {
        #[serde(borrow)]
        required: &'a str,
        optional: Option<i64>,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("required"),
        Data::String(AtprotoStr::String("value")),
    );
    // optional field not present
    let data = Data::Object(Object(map));

    let result: WithOption = from_data(&data).unwrap();
    assert_eq!(result.required, "value");
    assert_eq!(result.optional, None);
}

#[test]
fn test_borrowed_string_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct BorrowTest<'a> {
        #[serde(borrow)]
        text: &'a str,
    }

    // Use borrowed CowStr explicitly
    let borrowed_str = "borrowed text";
    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String(CowStr::Borrowed(borrowed_str))),
    );
    let data = Data::Object(Object(map));

    let result: BorrowTest = from_data(&data).unwrap();
    assert_eq!(result.text, "borrowed text");

    // Verify the borrowed string has the same address (zero-copy)
    assert_eq!(result.text.as_ptr(), borrowed_str.as_ptr());
}

#[test]
fn test_atproto_types_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct AtprotoTypes<'a> {
        #[serde(borrow)]
        did: Did<CowStr<'a>>,
        handle: Handle<CowStr<'a>>,
        cid: Cid<CowStr<'a>>,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("did"),
        Data::String(AtprotoStr::Did(Did::new_owned("did:plc:abc123").unwrap())),
    );
    map.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::Handle(
            Handle::new_static("alice.bsky.social").unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("cid"),
        Data::String(AtprotoStr::Cid(Cid::cow_str(CowStr::Borrowed(
            "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha",
        )))),
    );
    let data = Data::Object(Object(map));

    let result: AtprotoTypes = from_data(&data).unwrap();
    assert_eq!(result.did.as_str(), "did:plc:abc123");
    assert_eq!(result.handle.as_str(), "alice.bsky.social");
    assert_eq!(
        result.cid.as_str(),
        "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"
    );
}

#[test]
fn test_datetime_and_nsid_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct MixedTypes<'a> {
        #[serde(borrow)]
        nsid: Nsid<CowStr<'a>>,
        handle: Handle<CowStr<'a>>,
        did: Did<CowStr<'a>>,
        // These use SmolStr internally, so they allocate but still deserialize fine
        tid: Tid,
        created_at: Datetime,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("nsid"),
        Data::String(AtprotoStr::Nsid(
            Nsid::<CowStr<'_>>::new_static("app.bsky.feed.post").unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::Handle(
            Handle::new_static("alice.bsky.social").unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("did"),
        Data::String(AtprotoStr::Did(Did::new_owned("did:plc:test123").unwrap())),
    );
    map.insert(
        SmolStr::new_static("tid"),
        Data::String(AtprotoStr::Tid(Tid::new("3jzfcijpj2z2a").unwrap())),
    );
    map.insert(
        SmolStr::new_static("created_at"),
        Data::String(AtprotoStr::Datetime(
            Datetime::from_str("2024-01-15T12:30:45.123456Z").unwrap(),
        )),
    );
    let data = Data::Object(Object(map));

    let result: MixedTypes = from_data(&data).unwrap();
    assert_eq!(result.nsid.as_str(), "app.bsky.feed.post");
    assert_eq!(result.handle.as_str(), "alice.bsky.social");
    assert_eq!(result.did.as_str(), "did:plc:test123");
    assert_eq!(result.tid.as_str(), "3jzfcijpj2z2a");
    assert_eq!(result.created_at.as_str(), "2024-01-15T12:30:45.123456Z");
}

#[test]
fn test_aturi_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WithAtUri<'a> {
        #[serde(borrow)]
        uri: AtUri<CowStr<'a>>,
        did: Did<CowStr<'a>>,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("uri"),
        Data::String(AtprotoStr::AtUri(
            AtUri::new("at://alice.bsky.social/app.bsky.feed.post/3jk5".to_cowstr()).unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("did"),
        Data::String(AtprotoStr::Did(Did::new_owned("did:plc:test").unwrap())),
    );
    let data = Data::Object(Object(map));

    let result: WithAtUri = from_data(&data).unwrap();
    assert_eq!(
        result.uri.as_str(),
        "at://alice.bsky.social/app.bsky.feed.post/3jk5"
    );
    assert_eq!(result.did.as_str(), "did:plc:test");
}

#[test]
fn test_aturi_zero_copy_borrowed() {
    // AtUri<&str> should point at the original buffer.
    let uri_str = "at://alice.bsky.social/app.bsky.feed.post/3jk5";
    let uri = AtUri::new(uri_str).unwrap();
    assert_eq!(uri.as_str().as_ptr(), uri_str.as_ptr());
}

#[test]
fn test_aturi_zero_copy_cowstr() {
    // AtUri<CowStr<'a>> with a borrowed CowStr should point at the original buffer.
    let uri_str = "at://alice.bsky.social/app.bsky.feed.post/3jk5";
    let uri = AtUri::new(CowStr::Borrowed(uri_str)).unwrap();
    assert_eq!(uri.as_str().as_ptr(), uri_str.as_ptr());
}

#[test]
fn test_aturi_owned_allocates() {
    // AtUri<SmolStr> allocates — pointers should NOT match.
    let uri_str = "at://alice.bsky.social/app.bsky.feed.post/3jk5";
    let uri: AtUri<SmolStr> = AtUri::new_owned(uri_str).unwrap();
    assert_ne!(uri.as_str().as_ptr(), uri_str.as_ptr());
}

#[test]
fn test_atidentifier_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WithIdentifiers<'a> {
        #[serde(borrow)]
        ident_did: AtIdentifier<CowStr<'a>>,
        ident_handle: AtIdentifier<CowStr<'a>>,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("ident_did"),
        Data::String(AtprotoStr::AtIdentifier(AtIdentifier::Did(
            Did::new("did:plc:abc").unwrap(),
        ))),
    );
    map.insert(
        SmolStr::new_static("ident_handle"),
        Data::String(AtprotoStr::AtIdentifier(AtIdentifier::Handle(
            Handle::new("bob.test").unwrap(),
        ))),
    );
    let data = Data::Object(Object(map));

    let result: WithIdentifiers = from_data(&data).unwrap();
    match &result.ident_did {
        AtIdentifier::Did(did) => assert_eq!(did.as_str(), "did:plc:abc"),
        _ => panic!("expected Did variant"),
    }
    match &result.ident_handle {
        AtIdentifier::Handle(handle) => assert_eq!(handle.as_str(), "bob.test"),
        _ => panic!("expected Handle variant"),
    }
}

#[test]
fn test_json_value_deser() {
    // if this compiles, it works.
    let json = serde_json::json!({"name": "alice", "age": 30, "active": true});
    #[derive(Debug, serde::Deserialize)]
    struct TestStruct<'a> {
        #[serde(borrow)]
        name: CowStr<'a>,
        age: i64,
        active: bool,
    }

    impl IntoStatic for TestStruct<'_> {
        type Output = TestStruct<'static>;
        fn into_static(self) -> Self::Output {
            TestStruct {
                name: self.name.into_static(),
                age: self.age,
                active: self.active,
            }
        }
    }

    let _result = from_json_value::<TestStruct>(json).expect("should be right struct");
}

#[test]
fn test_to_raw_data() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestStruct {
        name: String,
        age: i64,
        active: bool,
    }

    let value = TestStruct {
        name: "alice".to_string(),
        age: 30,
        active: true,
    };

    let raw_data = to_raw_data(&value).unwrap();

    match raw_data {
        RawData::Object(map) => {
            assert_eq!(map.len(), 3);
            match map.get("name").unwrap() {
                RawData::String(s) => assert_eq!(s.as_ref(), "alice"),
                _ => panic!("expected string"),
            }
            match map.get("age").unwrap() {
                RawData::SignedInt(i) => assert_eq!(*i, 30),
                _ => panic!("expected signed int"),
            }
            match map.get("active").unwrap() {
                RawData::Boolean(b) => assert!(*b),
                _ => panic!("expected boolean"),
            }
        }
        _ => panic!("expected object"),
    }
}

#[test]
fn test_to_data_with_inference() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Post {
        text: String,
        author_did: String,
        created_at: String,
    }

    let post = Post {
        text: "hello world".to_string(),
        author_did: "did:plc:abc123".to_string(),
        created_at: "2024-01-15T12:30:45.123Z".to_string(),
    };

    let data: Data = to_data(&post).unwrap();

    match data {
        Data::Object(obj) => {
            // Check text is plain string
            match obj.0.get("text").unwrap() {
                Data::String(AtprotoStr::String(s)) => assert_eq!(s, "hello world"),
                _ => panic!("expected plain string for text"),
            }
            // Check DID was inferred
            match obj.0.get("author_did").unwrap() {
                Data::String(AtprotoStr::Did(did)) => assert_eq!(did.as_str(), "did:plc:abc123"),
                _ => panic!("expected Did type"),
            }
            // Check datetime was inferred
            match obj.0.get("created_at").unwrap() {
                Data::String(AtprotoStr::Datetime(dt)) => {
                    assert_eq!(dt.as_str(), "2024-01-15T12:30:45.123Z")
                }
                _ => panic!("expected Datetime type"),
            }
        }
        _ => panic!("expected object"),
    }
}

#[test]
fn test_option_vec_deserialization() {
    use serde::Deserialize;

    // Regression test for Option<Vec<T>> deserialization bug
    // Previously failed with "invalid type: sequence, expected option"
    #[derive(Debug, PartialEq, Deserialize)]
    struct WithOptionalArray<'a> {
        #[serde(borrow)]
        text: &'a str,
        langs: Option<Vec<Language>>,
        tags: Option<Vec<CowStr<'a>>>,
    }

    // Test with langs present
    let mut map_with_langs = BTreeMap::new();
    map_with_langs.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String("hello")),
    );
    map_with_langs.insert(
        SmolStr::new_static("langs"),
        Data::Array(Array(vec![
            Data::String(AtprotoStr::Language(Language::new("en").unwrap())),
            Data::String(AtprotoStr::Language(Language::new("fr").unwrap())),
        ])),
    );
    let data_with_langs = Data::Object(Object(map_with_langs));

    let result: WithOptionalArray = from_data(&data_with_langs).unwrap();
    assert_eq!(result.text, "hello");
    assert_eq!(result.langs.as_ref().map(|v| v.len()), Some(2));
    assert_eq!(result.tags, None);

    // Test with langs absent (None)
    let mut map_without_langs = BTreeMap::new();
    map_without_langs.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String("world")),
    );
    let data_without_langs = Data::Object(Object(map_without_langs));

    let result: WithOptionalArray = from_data(&data_without_langs).unwrap();
    assert_eq!(result.text, "world");
    assert_eq!(result.langs, None);
    assert_eq!(result.tags, None);

    // Test with null explicitly set
    let mut map_with_null = BTreeMap::new();
    map_with_null.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String("null test")),
    );
    map_with_null.insert(SmolStr::new_static("langs"), Data::Null);
    let data_with_null = Data::Object(Object(map_with_null));

    let result: WithOptionalArray = from_data(&data_with_null).unwrap();
    assert_eq!(result.text, "null test");
    assert_eq!(result.langs, None);
}

#[test]
fn test_data_accessors() {
    // Test as_object
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("key"), Data::<&str>::Integer(42));
    let obj_data = Data::Object(Object(map.clone()));
    assert!(obj_data.as_object().is_some());
    assert_eq!(obj_data.as_object().unwrap().0.len(), 1);
    assert!(Data::<&str>::Null.as_object().is_none());

    // Test as_array
    let arr_data = Data::<&str>::Array(Array(vec![Data::Integer(1), Data::Integer(2)]));
    assert!(arr_data.as_array().is_some());
    assert_eq!(arr_data.as_array().unwrap().0.len(), 2);
    assert!(Data::<&str>::Null.as_array().is_none());

    // Test as_str
    let str_data = Data::<&str>::String(AtprotoStr::String("hello".into()));
    assert_eq!(str_data.as_str(), Some("hello"));
    assert!(Data::<&str>::Null.as_str().is_none());

    // Test as_integer
    let int_data: Data = Data::Integer(42);
    assert_eq!(int_data.as_integer(), Some(42));
    assert!(Data::<&str>::Null.as_integer().is_none());

    // Test as_boolean
    let bool_data: Data = Data::Boolean(true);
    assert_eq!(bool_data.as_boolean(), Some(true));
    assert!(Data::<&str>::Null.as_boolean().is_none());

    // Test is_null
    assert!(Data::<&str>::Null.is_null());
    assert!(!Data::<&str>::Integer(0).is_null());
}

#[test]
fn test_rawdata_accessors() {
    // Test as_object
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("key"), RawData::SignedInt(42));
    let obj_data = RawData::Object(map.clone());
    assert!(obj_data.as_object().is_some());
    assert_eq!(obj_data.as_object().unwrap().len(), 1);
    assert!(RawData::Null.as_object().is_none());

    // Test as_array
    let arr_data = RawData::Array(vec![RawData::SignedInt(1), RawData::SignedInt(2)]);
    assert!(arr_data.as_array().is_some());
    assert_eq!(arr_data.as_array().unwrap().len(), 2);
    assert!(RawData::Null.as_array().is_none());

    // Test as_str
    let str_data = RawData::String("hello".into());
    assert_eq!(str_data.as_str(), Some("hello"));
    assert!(RawData::Null.as_str().is_none());

    // Test as_boolean
    let bool_data = RawData::Boolean(true);
    assert_eq!(bool_data.as_boolean(), Some(true));
    assert!(RawData::Null.as_boolean().is_none());

    // Test is_null
    assert!(RawData::Null.is_null());
    assert!(!RawData::SignedInt(0).is_null());
}

#[test]
fn test_data_to_dag_cbor() {
    // Test simple types
    let null_data: Data = Data::Null;
    assert!(null_data.to_dag_cbor().is_ok());

    let int_data: Data = Data::Integer(42);
    assert!(int_data.to_dag_cbor().is_ok());

    let str_data: Data = Data::String(AtprotoStr::String("hello".into()));
    assert!(str_data.to_dag_cbor().is_ok());

    // Test complex types
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("num"), Data::<SmolStr>::Integer(42));
    map.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String("test".into())),
    );
    let obj_data = Data::Object(Object(map));
    let cbor_result = obj_data.to_dag_cbor();
    assert!(cbor_result.is_ok());
    assert!(!cbor_result.unwrap().is_empty());

    // Test array
    let arr_data: Data = Data::Array(Array(vec![
        Data::Integer(1),
        Data::Integer(2),
        Data::Integer(3),
    ]));
    let arr_cbor = arr_data.to_dag_cbor();
    assert!(arr_cbor.is_ok());
    assert!(!arr_cbor.unwrap().is_empty());
}

#[test]
fn test_rawdata_to_dag_cbor() {
    // Test simple types
    let null_data = RawData::Null;
    assert!(null_data.to_dag_cbor().is_ok());

    let int_data = RawData::SignedInt(42);
    assert!(int_data.to_dag_cbor().is_ok());

    let str_data = RawData::String("hello".into());
    assert!(str_data.to_dag_cbor().is_ok());

    // Test complex types
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("num"), RawData::SignedInt(42));
    map.insert(SmolStr::new_static("text"), RawData::String("test".into()));
    let obj_data = RawData::Object(map);
    let cbor_result = obj_data.to_dag_cbor();
    assert!(cbor_result.is_ok());
    assert!(!cbor_result.unwrap().is_empty());
}

#[test]
fn test_object_methods() {
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("num"), Data::<SmolStr>::Integer(42));
    map.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String("hello".into())),
    );
    let obj = Object(map);

    // Test get
    assert!(obj.get("num").is_some());
    assert_eq!(obj.get("num"), Some(&Data::Integer(42)));
    assert!(obj.get("missing").is_none());

    // Test contains_key
    assert!(obj.contains_key("num"));
    assert!(!obj.contains_key("missing"));

    // Test len/is_empty
    assert_eq!(obj.len(), 2);
    assert!(!obj.is_empty());

    let empty_obj: Object = Object(BTreeMap::new());
    assert_eq!(empty_obj.len(), 0);
    assert!(empty_obj.is_empty());

    // Test indexing
    assert_eq!(&obj["num"], &Data::Integer(42));

    // Test iterators
    assert_eq!(obj.keys().count(), 2);
    assert_eq!(obj.values().count(), 2);
    assert_eq!(obj.iter().count(), 2);
}

#[test]
fn test_array_methods() {
    let arr = Array::<&str>(vec![Data::Integer(1), Data::Integer(2), Data::Integer(3)]);

    // Test get
    assert_eq!(arr.get(0), Some(&Data::Integer(1)));
    assert_eq!(arr.get(2), Some(&Data::Integer(3)));
    assert!(arr.get(3).is_none());

    // Test len/is_empty
    assert_eq!(arr.len(), 3);
    assert!(!arr.is_empty());

    let empty_arr = Array::<&str>(vec![]);
    assert_eq!(empty_arr.len(), 0);
    assert!(empty_arr.is_empty());

    // Test indexing
    assert_eq!(&arr[1], &Data::Integer(2));

    // Test iterator
    assert_eq!(arr.iter().count(), 3);
}

#[test]
fn test_get_at_path_simple() {
    // Build nested structure: {"embed": {"alt": "test"}}
    let mut inner = BTreeMap::new();
    inner.insert(
        SmolStr::new_static("alt"),
        Data::String(AtprotoStr::String("test")),
    );

    let mut outer = BTreeMap::new();
    outer.insert(SmolStr::new_static("embed"), Data::Object(Object(inner)));

    let data = Data::Object(Object(outer));

    // Test simple field access
    let result = data.get_at_path("embed.alt");
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), Some("test"));

    // Test with leading dot
    let result2 = data.get_at_path(".embed.alt");
    assert!(result2.is_some());
    assert_eq!(result2.unwrap().as_str(), Some("test"));

    // Test missing path
    assert!(data.get_at_path("missing.field").is_none());
    assert!(data.get_at_path("embed.missing").is_none());
}

#[test]
fn test_get_at_path_arrays() {
    // Build: {"items": [{"name": "first"}, {"name": "second"}]}
    let mut item1 = BTreeMap::new();
    item1.insert(
        SmolStr::new_static("name"),
        Data::String(AtprotoStr::String("first")),
    );

    let mut item2 = BTreeMap::new();
    item2.insert(
        SmolStr::new_static("name"),
        Data::String(AtprotoStr::String("second")),
    );

    let items = Data::Array(Array(vec![
        Data::Object(Object(item1)),
        Data::Object(Object(item2)),
    ]));

    let mut outer = BTreeMap::new();
    outer.insert(SmolStr::new_static("items"), items);
    let data = Data::Object(Object(outer));

    // Test array indexing
    let result = data.get_at_path("items[0].name");
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), Some("first"));

    let result2 = data.get_at_path("items[1].name");
    assert!(result2.is_some());
    assert_eq!(result2.unwrap().as_str(), Some("second"));

    // Test out of bounds
    assert!(data.get_at_path("items[2].name").is_none());
}

#[test]
fn test_get_at_path_complex() {
    // Build: {"post": {"embed": {"images": [{"alt": "img1"}, {"alt": "img2"}]}}}
    let mut img1 = BTreeMap::new();
    img1.insert(
        SmolStr::new_static("alt"),
        Data::String(AtprotoStr::String("img1")),
    );

    let mut img2 = BTreeMap::new();
    img2.insert(
        SmolStr::new_static("alt"),
        Data::String(AtprotoStr::String("img2")),
    );

    let images = Data::Array(Array(vec![
        Data::Object(Object(img1)),
        Data::Object(Object(img2)),
    ]));

    let mut embed_map = BTreeMap::new();
    embed_map.insert(SmolStr::new_static("images"), images);

    let mut post_map = BTreeMap::new();
    post_map.insert(
        SmolStr::new_static("embed"),
        Data::Object(Object(embed_map)),
    );

    let mut root = BTreeMap::new();
    root.insert(SmolStr::new_static("post"), Data::Object(Object(post_map)));

    let data = Data::Object(Object(root));

    // Test complex nested path
    let result = data.get_at_path("post.embed.images[1].alt");
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), Some("img2"));
}

#[test]
fn test_rawdata_get_at_path() {
    // Build nested RawData structure
    let mut inner = BTreeMap::new();
    inner.insert(SmolStr::new_static("value"), RawData::SignedInt(42));

    let mut outer = BTreeMap::new();
    outer.insert(SmolStr::new_static("nested"), RawData::Object(inner));

    let data = RawData::Object(outer);

    // Test simple field access
    let result = data.get_at_path("nested.value");
    assert!(result.is_some());
    if let Some(RawData::SignedInt(n)) = result {
        assert_eq!(*n, 42);
    } else {
        panic!("Expected SignedInt");
    }
}

#[test]
fn test_query_exact_path() {
    let mut inner = BTreeMap::new();
    inner.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::String("alice.bsky.social")),
    );

    let mut outer = BTreeMap::new();
    outer.insert(SmolStr::new_static("author"), Data::Object(Object(inner)));

    let data = Data::Object(Object(outer));

    // Exact path should return Single
    let result = data.query("author.handle");
    assert!(matches!(result, QueryResult::Single(_)));
    assert_eq!(result.single().unwrap().as_str(), Some("alice.bsky.social"));
}

#[test]
fn test_query_wildcard_array() {
    // Build: {"actors": [{"handle": "alice"}, {"handle": "bob"}, {"name": "carol"}]}
    let mut actor1 = BTreeMap::new();
    actor1.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::String("alice")),
    );

    let mut actor2 = BTreeMap::new();
    actor2.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::String("bob")),
    );

    let mut actor3 = BTreeMap::new();
    actor3.insert(
        SmolStr::new_static("name"),
        Data::String(AtprotoStr::String("carol")),
    );

    let actors = Data::Array(Array(vec![
        Data::Object(Object(actor1)),
        Data::Object(Object(actor2)),
        Data::Object(Object(actor3)),
    ]));

    let mut root = BTreeMap::new();
    root.insert(SmolStr::new_static("actors"), actors);
    let data = Data::Object(Object(root));

    // Wildcard over array
    let result = data.query("actors.[..]");
    assert!(matches!(result, QueryResult::Multiple(_)));
    let matches = result.multiple().unwrap();
    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].path.as_str(), "actors[0]");
    assert_eq!(matches[1].path.as_str(), "actors[1]");
    assert_eq!(matches[2].path.as_str(), "actors[2]");
}

#[test]
fn test_query_wildcard_object() {
    // Build: {"embed": {"images": {...}, "video": {...}}}
    let mut images = BTreeMap::new();
    images.insert(
        SmolStr::new_static("alt"),
        Data::String(AtprotoStr::String("img")),
    );

    let mut video = BTreeMap::new();
    video.insert(
        SmolStr::new_static("alt"),
        Data::String(AtprotoStr::String("vid")),
    );

    let mut embed = BTreeMap::new();
    embed.insert(SmolStr::new_static("images"), Data::Object(Object(images)));
    embed.insert(SmolStr::new_static("video"), Data::Object(Object(video)));

    let mut root = BTreeMap::new();
    root.insert(SmolStr::new_static("embed"), Data::Object(Object(embed)));
    let data = Data::Object(Object(root));

    // Wildcard over object values
    let result = data.query("embed.[..]");
    assert!(matches!(result, QueryResult::Multiple(_)));
    let matches = result.multiple().unwrap();
    assert_eq!(matches.len(), 2); // images and video
}

#[test]
fn test_query_scoped_recursion() {
    // Build: {"post": {"author": {"profile": {"handle": "alice"}}}}
    let mut handle_map = BTreeMap::new();
    handle_map.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::String("alice")),
    );

    let mut profile_map = BTreeMap::new();
    profile_map.insert(
        SmolStr::new_static("profile"),
        Data::Object(Object(handle_map)),
    );

    let mut author_map = BTreeMap::new();
    author_map.insert(
        SmolStr::new_static("author"),
        Data::Object(Object(profile_map)),
    );

    let mut post_map = BTreeMap::new();
    post_map.insert(
        SmolStr::new_static("post"),
        Data::Object(Object(author_map)),
    );

    let data = Data::Object(Object(post_map));

    // Scoped recursion: find handle within post
    let result = data.query("post..handle");
    assert!(matches!(result, QueryResult::Single(_)));
    assert_eq!(result.single().unwrap().as_str(), Some("alice"));
    assert_eq!(result.first().unwrap().as_str(), Some("alice"));
}

#[test]
fn test_query_global_recursion() {
    // Build structure with multiple 'cid' fields at different depths
    let mut inner1 = BTreeMap::new();
    inner1.insert(
        SmolStr::new_static("cid"),
        Data::String(AtprotoStr::String("cid1")),
    );

    let mut inner2 = BTreeMap::new();
    inner2.insert(
        SmolStr::new_static("cid"),
        Data::String(AtprotoStr::String("cid2")),
    );

    let mut middle = BTreeMap::new();
    middle.insert(SmolStr::new_static("post"), Data::Object(Object(inner1)));
    middle.insert(SmolStr::new_static("reply"), Data::Object(Object(inner2)));

    let mut root = BTreeMap::new();
    root.insert(SmolStr::new_static("thread"), Data::Object(Object(middle)));
    root.insert(
        SmolStr::new_static("cid"),
        Data::String(AtprotoStr::String("cid3")),
    );

    let data = Data::Object(Object(root));

    // Global recursion: find all cids
    let result = data.query("...cid");
    assert!(matches!(result, QueryResult::Multiple(_)));
    let matches = result.multiple().unwrap();
    assert_eq!(matches.len(), 3);

    // Check values
    let values: Vec<_> = result.values().map(|d| d.as_str().unwrap()).collect();
    assert!(values.contains(&"cid1"));
    assert!(values.contains(&"cid2"));
    assert!(values.contains(&"cid3"));
}

#[test]
fn test_query_combined_wildcard_field() {
    // Build: {"actors": [{"handle": "alice"}, {"handle": "bob"}]}
    let mut actor1 = BTreeMap::new();
    actor1.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::String("alice")),
    );

    let mut actor2 = BTreeMap::new();
    actor2.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::String("bob")),
    );

    let actors = Data::Array(Array(vec![
        Data::Object(Object(actor1)),
        Data::Object(Object(actor2)),
    ]));

    let mut root = BTreeMap::new();
    root.insert(SmolStr::new_static("actors"), actors);
    let data = Data::Object(Object(root));

    // Wildcard + field: collect handle from each actor
    let result = data.query("actors.[..].handle");
    assert!(matches!(result, QueryResult::Multiple(_)));
    let matches = result.multiple().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].value.unwrap().as_str(), Some("alice"));
    assert_eq!(matches[1].value.unwrap().as_str(), Some("bob"));
}

#[test]
fn test_query_no_match() {
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("foo"), Data::<&str>::Integer(42));
    let data = Data::Object(Object(map));

    // Field doesn't exist
    let result = data.query("missing");
    assert!(matches!(result, QueryResult::None));
    assert!(result.is_empty());
    assert!(result.single().is_none());
    assert!(result.first().is_none());
}

#[test]
fn test_query_result_helpers() {
    let mut map = BTreeMap::new();
    map.insert(SmolStr::new_static("value"), Data::<&str>::Integer(42));
    let data = Data::Object(Object(map));

    let result = data.query("value");

    // Test helper methods
    assert!(!result.is_empty());
    assert!(result.single().is_some());
    assert_eq!(result.first().unwrap().as_integer(), Some(42));

    let values: Vec<_> = result.values().collect();
    assert_eq!(values.len(), 1);
}

#[test]
fn test_type_discriminator() {
    // Object with $type field
    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("$type"),
        Data::String(AtprotoStr::String(CowStr::new_static("app.bsky.feed.post"))),
    );
    map.insert(
        SmolStr::new_static("text"),
        Data::String(AtprotoStr::String(CowStr::new_static("hello"))),
    );
    let obj = Object(map);

    assert_eq!(obj.type_discriminator(), Some("app.bsky.feed.post"));

    let data = Data::Object(obj.clone());
    assert_eq!(data.type_discriminator(), Some("app.bsky.feed.post"));

    // Object without $type field
    let mut map2 = BTreeMap::new();
    map2.insert(SmolStr::new_static("foo"), Data::<&str>::Integer(42));
    let obj2 = Object(map2);

    assert_eq!(obj2.type_discriminator(), None);

    let data2 = Data::Object(obj2);
    assert_eq!(data2.type_discriminator(), None);

    // Non-object data
    let data3: Data = Data::Integer(42);
    assert_eq!(data3.type_discriminator(), None);

    // RawData with $type
    let mut raw_map = BTreeMap::new();
    raw_map.insert(
        SmolStr::new_static("$type"),
        RawData::String(CowStr::new_static("test.type")),
    );
    let raw_obj = RawData::Object(raw_map);

    assert_eq!(raw_obj.type_discriminator(), Some("test.type"));
}

#[test]
fn parse_string_nsid_with_camelcase_name() {
    use super::parsing::parse_string;
    use crate::types::string::AtprotoStr;

    // NSIDs with camelCase names (uppercase in last segment) are unambiguously NSIDs.
    let nsid = parse_string("com.atproto.repo.getRecord");
    assert!(
        matches!(nsid, AtprotoStr::Nsid(_)),
        "NSID was misclassified: {:?}",
        nsid
    );

    let nsid2 = parse_string("com.atproto.sync.subscribeRepos");
    assert!(
        matches!(nsid2, AtprotoStr::Nsid(_)),
        "NSID was misclassified: {:?}",
        nsid2
    );

    // "app.bsky.feed.post" starts with "app" (a known TLD) → reverse domain order → NSID.
    let nsid3 = parse_string("app.bsky.feed.post");
    assert!(
        matches!(nsid3, AtprotoStr::Nsid(_)),
        "NSID was misclassified: {:?}",
        nsid3
    );
}

#[test]
fn parse_string_handle_classified_correctly() {
    use super::parsing::parse_string;
    use crate::types::string::AtprotoStr;

    let handle = parse_string("example.bsky.social");
    assert!(
        matches!(handle, AtprotoStr::AtIdentifier(_)),
        "handle was misclassified: {:?}",
        handle
    );

    let handle2 = parse_string("jay.bsky");
    assert!(
        matches!(handle2, AtprotoStr::AtIdentifier(_)),
        "two-segment handle misclassified: {:?}",
        handle2
    );
}

#[test]
fn parse_string_https_uri() {
    use super::parsing::parse_string;
    use crate::types::string::AtprotoStr;
    use crate::types::uri::UriValue;

    let uri = parse_string("https://example.com/path");
    assert!(
        matches!(uri, AtprotoStr::Uri(UriValue::Https(_))),
        "https URI was misclassified: {:?}",
        uri
    );
}

#[test]
fn parse_string_wss_uri() {
    use super::parsing::parse_string;
    use crate::types::string::AtprotoStr;
    use crate::types::uri::UriValue;

    let uri = parse_string("wss://bsky.network/subscribe");
    assert!(
        matches!(uri, AtprotoStr::Uri(UriValue::Wss(_))),
        "wss URI was misclassified: {:?}",
        uri
    );
}

#[test]
fn parse_string_disambiguation_edge_cases() {
    use super::parsing::parse_string;
    use crate::types::string::AtprotoStr;

    // Both first and last segments are TLDs — first-is-TLD wins → NSID.
    let both_tld = parse_string("com.net.service");
    assert!(
        matches!(both_tld, AtprotoStr::Nsid(_)),
        "both-TLD case: {:?}",
        both_tld
    );

    // Neither segment is a known TLD — falls through to fallback.
    // "foo.bar.baz" is valid as both a handle and an NSID; handle wins in fallback.
    let neither_tld = parse_string("foo.bar.baz");
    // Handle validation may reject this (handle TLD must be ≥2 alpha chars, which "baz" satisfies).
    // If handle passes, it wins; if not, NSID fallback.
    assert!(
        matches!(
            neither_tld,
            AtprotoStr::AtIdentifier(_) | AtprotoStr::Nsid(_)
        ),
        "neither-TLD case should be handle or NSID: {:?}",
        neither_tld
    );

    // Two-segment where both are TLDs (e.g., "example.social").
    // First segment "example" is not a TLD, "social" is → handle.
    let two_seg = parse_string("example.social");
    assert!(
        matches!(two_seg, AtprotoStr::AtIdentifier(_)),
        "two-segment handle: {:?}",
        two_seg
    );
}
