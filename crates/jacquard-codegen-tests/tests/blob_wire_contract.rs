// Deterministic wire and validation contracts for generated blob-bearing
// bindings, in both Pretty and Macro codegen modes, plus one real
// jacquard-api binding to pin production generator output.
extern crate alloc;

use ipld_core::ipld::Ipld;
use jacquard_codegen_tests::{macro_mode, pretty};
use jacquard_common::types::blob::{Blob, BlobRef};
use jacquard_common::{CowStr, DefaultStr};
use jacquard_lexicon::schema::LexiconSchema;
use jacquard_lexicon::validation::ConstraintError;

const PNG_CID: &str = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";

fn png_blob() -> BlobRef<DefaultStr> {
    serde_json::from_str(&format!(
        r#"{{"$type":"blob","ref":{{"$link":"{PNG_CID}"}},"mimeType":"image/png","size":1000}}"#
    ))
    .expect("parse png blob")
}

fn blob_mut(blob_ref: &mut BlobRef<DefaultStr>) -> &mut Blob<DefaultStr> {
    match blob_ref {
        BlobRef::Blob(blob) => blob,
    }
}

fn pretty_record() -> pretty::test_blobby::record::Record<DefaultStr> {
    pretty::test_blobby::record::Record {
        primary: png_blob(),
        secondary: None,
        note: Some(DefaultStr::new("contract")),
        extra_data: None,
    }
}

fn macro_record() -> macro_mode::test_blobby::record::Record<DefaultStr> {
    macro_mode::test_blobby::record::Record {
        primary: png_blob(),
        secondary: None,
        note: Some(DefaultStr::new("contract")),
        extra_data: None,
    }
}

#[test]
fn pretty_generated_blob_json_wire_contract() {
    let json = serde_json::to_value(pretty_record()).expect("serialize pretty record");
    assert_eq!(
        json,
        serde_json::json!({
            "$type": "test.blobby.record",
            "primary": {
                "$type": "blob",
                "ref": { "$link": PNG_CID },
                "mimeType": "image/png",
                "size": 1000,
            },
            "note": "contract",
        }),
        "exact AT Protocol JSON blob shape with no leaked or dropped fields"
    );
    // Optional blob omission: `secondary` absent, not null.
    assert!(json.get("secondary").is_none());
}

#[test]
fn macro_generated_blob_json_wire_contract() {
    let json = serde_json::to_value(macro_record()).expect("serialize macro record");
    assert_eq!(
        json,
        serde_json::json!({
            "$type": "test.blobby.record",
            "primary": {
                "$type": "blob",
                "ref": { "$link": PNG_CID },
                "mimeType": "image/png",
                "size": 1000,
            },
            "note": "contract",
        }),
        "macro mode must produce the identical wire shape"
    );
}

#[test]
fn generated_blob_dag_cbor_wire_contract() {
    let bytes = serde_ipld_dagcbor::to_vec(&pretty_record()).expect("encode dag-cbor");
    let ipld: Ipld = serde_ipld_dagcbor::from_slice(&bytes).expect("decode dag-cbor");

    let Ipld::Map(map) = ipld else {
        panic!("record must encode as a CBOR map");
    };
    // In CBOR the blob `ref` is the raw CID link, not a {"$link": ...} wrapper.
    let primary = match map.get("primary") {
        Some(Ipld::Map(primary)) => primary.clone(),
        other => panic!("primary must be a map, got {other:?}"),
    };
    let Ipld::Link(cid) = primary.get("ref").expect("blob ref present") else {
        panic!("dag-cbor blob ref must be a CID link");
    };
    assert_eq!(cid.to_string(), PNG_CID);
    assert_eq!(
        primary.get("mimeType"),
        Some(&Ipld::String("image/png".into()))
    );
    assert_eq!(primary.get("size"), Some(&Ipld::Integer(1000)));
    assert_eq!(
        map.get("$type"),
        Some(&Ipld::String("test.blobby.record".into()))
    );

    // Typed round-trip plus byte-stable re-encode. The CBOR decode yields the
    // parsed `Ipld` CID variant while the JSON-parsed fixture uses `Str`, so
    // equality is asserted on the semantic fields, not the representation.
    let decoded: pretty::test_blobby::record::Record<DefaultStr> =
        serde_ipld_dagcbor::from_slice(&bytes).expect("typed dag-cbor decode");
    assert_eq!(decoded.note.as_deref(), Some("contract"));
    assert!(decoded.secondary.is_none());
    assert_eq!(decoded.primary.blob().cid().as_str(), PNG_CID);
    assert_eq!(decoded.primary.blob().mime_type.as_str(), "image/png");
    assert_eq!(decoded.primary.blob().size, 1000);
    let reencoded = serde_ipld_dagcbor::to_vec(&decoded).expect("re-encode dag-cbor");
    assert_eq!(reencoded, bytes, "dag-cbor encoding must be byte-stable");
}

#[test]
fn generated_blob_backing_type_round_trips() {
    let json = serde_json::to_string(&pretty_record()).expect("serialize");

    // Owned (DefaultStr) round-trip.
    let owned: pretty::test_blobby::record::Record<DefaultStr> =
        serde_json::from_str(&json).expect("owned parse");
    assert_eq!(owned, pretty_record());

    // Borrow-or-share round-trip.
    let borrowed: pretty::test_blobby::record::Record<CowStr<'_>> =
        serde_json::from_str(&json).expect("borrowed parse");
    assert_eq!(borrowed.primary.blob().size, 1000);
    assert_eq!(borrowed.primary.blob().mime_type.as_str(), "image/png");
    assert_eq!(
        serde_json::to_string(&borrowed).expect("borrowed serialize"),
        json
    );
}

#[test]
fn generated_blob_optional_omission_is_distinct_from_invalid() {
    // Omitted optional blob deserializes to None and validates cleanly.
    let json = serde_json::json!({
        "$type": "test.blobby.record",
        "primary": {
            "$type": "blob",
            "ref": { "$link": PNG_CID },
            "mimeType": "image/png",
            "size": 10,
        },
    });
    let record: pretty::test_blobby::record::Record<DefaultStr> =
        serde_json::from_value(json).expect("parse without secondary");
    assert!(record.secondary.is_none());
    assert!(record.validate().is_ok(), "omission is not invalidity");

    // A present-but-invalid optional blob must fail validation.
    let json = serde_json::json!({
        "$type": "test.blobby.record",
        "primary": {
            "$type": "blob",
            "ref": { "$link": PNG_CID },
            "mimeType": "image/png",
            "size": 10,
        },
        "secondary": {
            "$type": "blob",
            "ref": { "$link": PNG_CID },
            "mimeType": "text/plain",
            "size": 5000,
        },
    });
    let record: pretty::test_blobby::record::Record<DefaultStr> =
        serde_json::from_value(json).expect("parse invalid secondary");
    assert!(record.secondary.is_some());
    assert!(
        record.validate().is_err(),
        "present invalid blob is invalid"
    );
}

#[test]
fn generated_blob_constraints_report_field_path() {
    let mut record = pretty_record();

    // Boundary: exactly maxSize is accepted, one byte over is rejected.
    blob_mut(&mut record.primary).size = 1000;
    assert!(record.validate().is_ok());
    blob_mut(&mut record.primary).size = 1001;
    match record.validate() {
        Err(ConstraintError::BlobTooLarge { path, max, actual }) => {
            assert_eq!(path.to_string(), ".primary");
            assert_eq!((max, actual), (1000, 1001));
        }
        other => panic!("expected BlobTooLarge on primary, got {other:?}"),
    }

    // Boundary MIME accepted by pattern/exact list, wrong MIME rejected with path.
    blob_mut(&mut record.primary).size = 1000;
    blob_mut(&mut record.primary).mime_type =
        jacquard_common::types::blob::MimeType::new_owned("image/jpeg");
    assert!(record.validate().is_ok());
    blob_mut(&mut record.primary).mime_type =
        jacquard_common::types::blob::MimeType::new_owned("image/webp");
    match record.validate() {
        Err(ConstraintError::BlobMimeTypeNotAccepted { path, actual, .. }) => {
            assert_eq!(path.to_string(), ".primary");
            assert_eq!(actual, "image/webp");
        }
        other => panic!("expected BlobMimeTypeNotAccepted on primary, got {other:?}"),
    }

    // The optional blob reports under its own field path. Its `*/*` accept
    // pattern matches every MIME type, so exercise the size constraint.
    blob_mut(&mut record.primary).mime_type =
        jacquard_common::types::blob::MimeType::new_owned("image/png");
    record.secondary = Some(
        serde_json::from_str(&format!(
            r#"{{"$type":"blob","ref":{{"$link":"{PNG_CID}"}},"mimeType":"text/plain","size":5000}}"#
        ))
        .expect("parse secondary"),
    );
    match record.validate() {
        Err(ConstraintError::BlobTooLarge { path, max, actual }) => {
            assert_eq!(path.to_string(), ".secondary");
            assert_eq!((max, actual), (4000, 5000));
        }
        other => panic!("expected BlobTooLarge on secondary, got {other:?}"),
    }
    record.secondary = None;
    assert!(record.validate().is_ok());
}

#[test]
fn real_api_blob_binding_wire_contract() {
    use jacquard_api::app_bsky::embed::images::{Image, Images};

    let image = Image {
        image: png_blob(),
        alt: DefaultStr::new("real binding contract"),
        aspect_ratio: None,
        extra_data: None,
    };

    let json = serde_json::to_value(&image).expect("serialize real binding");
    assert_eq!(
        json,
        serde_json::json!({
            "image": {
                "$type": "blob",
                "ref": { "$link": PNG_CID },
                "mimeType": "image/png",
                "size": 1000,
            },
            "alt": "real binding contract",
        }),
        "production generator output must match the curated fixture wire shape"
    );

    // Composition through the parent embed type, JSON and DAG-CBOR.
    let images = Images {
        images: vec![image],
        extra_data: None,
    };
    let images_json = serde_json::to_string(&images).expect("serialize images");
    let reparsed: Images<DefaultStr> = serde_json::from_str(&images_json).expect("parse images");
    // serde(flatten) deserializes an absent extra_data map to Some({}), so
    // compare on the wire representation and semantic fields, not derived Eq.
    assert_eq!(
        serde_json::to_string(&reparsed).expect("reserialize"),
        images_json
    );
    assert_eq!(reparsed.images.len(), 1);
    assert_eq!(reparsed.images[0].alt.as_str(), "real binding contract");
    assert_eq!(reparsed.images[0].image.blob().cid().as_str(), PNG_CID);

    let bytes = serde_ipld_dagcbor::to_vec(&images).expect("encode dag-cbor");
    let decoded: Images<DefaultStr> = serde_ipld_dagcbor::from_slice(&bytes).expect("decode");
    assert_eq!(decoded.images[0].image.blob().cid().as_str(), PNG_CID);
    assert_eq!(
        decoded.images[0].image.blob().mime_type.as_str(),
        "image/png"
    );
    assert_eq!(
        serde_ipld_dagcbor::to_vec(&decoded).expect("re-encode"),
        bytes,
        "dag-cbor encoding must be byte-stable"
    );

    // The real binding enforces the same blob constraint semantics. Nested
    // refs are not recursed by the parent's validate(); each validated type
    // owns its own constraints.
    let mut invalid = reparsed;
    let BlobRef::Blob(blob) = &mut invalid.images[0].image;
    blob.size = 2_000_001;
    match invalid.images[0].validate() {
        Err(ConstraintError::BlobTooLarge { path, .. }) => {
            assert_eq!(path.to_string(), ".image");
        }
        other => panic!("expected BlobTooLarge on real binding, got {other:?}"),
    }
}
