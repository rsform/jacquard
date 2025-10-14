use super::*;
use std::str::FromStr;

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
    let data = Data::CidLink(Cid::str(
        "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha",
    ));

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
            assert_eq!(s.as_ref(), "test");
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
#[ignore]
fn reject_floats() {
    let json = "42.5"; // float literal

    let result: Result<Data, _> = serde_json::from_str(json);
    assert!(result.is_err());
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
        Data::String(AtprotoStr::String("Alice".into())),
    );
    map.insert(SmolStr::new_static("age"), Data::Integer(30));
    let data = Data::Object(Object(map));

    let result: TestStruct = from_data(&data).unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.age, 30);
}

#[test]
fn test_from_data_vec() {
    let data = Data::Array(Array(vec![
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
        Data::String(AtprotoStr::String("test".into())),
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
        Data::String(AtprotoStr::String("value".into())),
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
        did: Did<'a>,
        handle: Handle<'a>,
        cid: Cid<'a>,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("did"),
        Data::String(AtprotoStr::Did(Did::new("did:plc:abc123").unwrap())),
    );
    map.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::Handle(
            Handle::new("alice.bsky.social").unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("cid"),
        Data::String(AtprotoStr::Cid(Cid::str(
            "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha",
        ))),
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
        nsid: Nsid<'a>,
        handle: Handle<'a>,
        did: Did<'a>,
        // These use SmolStr internally, so they allocate but still deserialize fine
        tid: Tid,
        created_at: Datetime,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("nsid"),
        Data::String(AtprotoStr::Nsid(Nsid::new("app.bsky.feed.post").unwrap())),
    );
    map.insert(
        SmolStr::new_static("handle"),
        Data::String(AtprotoStr::Handle(
            Handle::new("alice.bsky.social").unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("did"),
        Data::String(AtprotoStr::Did(Did::new("did:plc:test123").unwrap())),
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
        uri: AtUri<'a>,
        did: Did<'a>,
    }

    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("uri"),
        Data::String(AtprotoStr::AtUri(
            AtUri::new("at://alice.bsky.social/app.bsky.feed.post/3jk5").unwrap(),
        )),
    );
    map.insert(
        SmolStr::new_static("did"),
        Data::String(AtprotoStr::Did(Did::new("did:plc:test").unwrap())),
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
fn test_aturi_zero_copy() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WithAtUri<'a> {
        #[serde(borrow)]
        uri: AtUri<'a>,
    }

    // Use borrowed CowStr to create the AtUri
    let uri_str = "at://alice.bsky.social/app.bsky.feed.post/3jk5";
    let mut map = BTreeMap::new();
    map.insert(
        SmolStr::new_static("uri"),
        Data::String(AtprotoStr::AtUri(AtUri::new(uri_str).unwrap())),
    );
    let data = Data::Object(Object(map));

    let result: WithAtUri = from_data(&data).unwrap();

    // Check if the AtUri borrowed from the original string
    assert_eq!(result.uri.as_str().as_ptr(), uri_str.as_ptr());
}

#[test]
fn test_atidentifier_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WithIdentifiers<'a> {
        #[serde(borrow)]
        ident_did: AtIdentifier<'a>,
        ident_handle: AtIdentifier<'a>,
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

    let data = to_data(&post).unwrap();

    match data {
        Data::Object(obj) => {
            // Check text is plain string
            match obj.0.get("text").unwrap() {
                Data::String(AtprotoStr::String(s)) => assert_eq!(s.as_ref(), "hello world"),
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
