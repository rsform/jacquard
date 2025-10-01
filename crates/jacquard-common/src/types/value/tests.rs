use super::*;

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
    let data = Data::CidLink(Cid::str("bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"));

    // JSON: should be {"$link": "cid_string"}
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("$link"));
    assert!(json.contains("bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"));

    let parsed: Data = serde_json::from_str(&json).unwrap();
    match parsed {
        Data::CidLink(cid) => assert_eq!(cid.as_str(), "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"),
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
    map.insert("name".to_smolstr(), Data::String(AtprotoStr::String("alice".into())));
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

        assert_eq!(original_canonical, serialized_canonical, "Serialized JSON should match original structure")
    } else {
        panic!("expected top-level object");
    }
}
