use crate::{
    IntoStatic,
    types::{
        DataModelType, LexiconStringType, UriType,
        blob::{Blob, MimeType},
        string::*,
        value::{AtDataError, Data, RawData},
    },
};
use base64::{
    Engine,
    prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD, BASE64_URL_SAFE, BASE64_URL_SAFE_NO_PAD},
};
use bytes::Bytes;
use ipld_core::ipld::Ipld;
use smol_str::{SmolStr, ToSmolStr};
use std::{collections::BTreeMap, str::FromStr};
use url::Url;

/// Insert a string into an at:// `Data<'_>` map, inferring its type.
pub fn insert_string<'s>(
    map: &mut BTreeMap<SmolStr, Data<'s>>,
    key: &'s str,
    value: &'s str,
    string_type: LexiconStringType,
) -> Result<(), AtDataError> {
    match string_type {
        LexiconStringType::Datetime => {
            if let Ok(datetime) = Datetime::from_str(value) {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::Datetime(datetime)),
                );
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::AtUri => {
            if let Ok(value) = AtUri::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::AtUri(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Did => {
            if let Ok(value) = Did::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Did(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Handle => {
            if let Ok(value) = Handle::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Handle(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::AtIdentifier => {
            if let Ok(value) = AtIdentifier::new(value) {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::AtIdentifier(value)),
                );
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Nsid => {
            if let Ok(value) = Nsid::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Nsid(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Cid => {
            if let Ok(value) = Cid::new(value.as_bytes()) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Cid(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Language => {
            if let Ok(value) = Language::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Language(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Tid => {
            if let Ok(value) = Tid::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Tid(value)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::RecordKey => {
            if let Ok(value) = Rkey::new(value) {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::RecordKey(RecordKey::from(value))),
                );
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Uri(_) => {
            if let Ok(uri) = Uri::new(value) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Uri(uri)));
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::String => {
            map.insert(key.to_smolstr(), Data::String(parse_string(value)));
        }
    }
    Ok(())
}

/// smarter parsing to avoid trying as many posibilities.
pub fn parse_string<'s>(string: &'s str) -> AtprotoStr<'s> {
    if string.len() < 2048 && string.starts_with("did:") {
        if let Ok(did) = Did::new(string) {
            return AtprotoStr::Did(did);
        }
    } else if string.starts_with("20") && string.ends_with("Z") {
        // probably a date (for the next 75 years)
        if let Ok(datetime) = Datetime::from_str(string) {
            return AtprotoStr::Datetime(datetime);
        }
    } else if string.starts_with("at://") {
        if let Ok(uri) = AtUri::new(string) {
            return AtprotoStr::AtUri(uri);
        }
    } else if string.starts_with("https://") {
        if let Ok(uri) = Url::parse(string) {
            return AtprotoStr::Uri(Uri::Https(uri));
        }
    } else if string.starts_with("wss://") {
        if let Ok(uri) = Url::parse(string) {
            return AtprotoStr::Uri(Uri::Https(uri));
        }
    } else if string.starts_with("ipfs://") {
        return AtprotoStr::Uri(Uri::Cid(Cid::str(string)));
    } else if string.contains('.') && !string.contains([' ', '\n']) {
        if string.len() < 253 && Url::parse(string).is_ok() {
            // probably a handle
            if let Ok(handle) = AtIdentifier::new(string) {
                return AtprotoStr::AtIdentifier(handle);
            } else {
                return AtprotoStr::Uri(Uri::Any(string.into()));
            }
        } else if let Ok(nsid) = Nsid::new(string) {
            return AtprotoStr::Nsid(nsid);
        }
    } else if string.len() == 13 {
        if let Ok(tid) = Tid::new(string) {
            return AtprotoStr::Tid(tid);
        }
    } else if !string.contains([' ', '\n']) && string.len() > 20 {
        // CID: must be longer than typical short strings to avoid false positives
        // Most CIDs are 46+ chars (base32 encoded), minimum realistic is around 30
        if let Ok(cid) = Cid::new(string.as_bytes()) {
            return AtprotoStr::Cid(cid);
        }
    }

    AtprotoStr::String(string.into())
}

/// First-level guess at what we should parse the corresponding value as
/// Helps speed up parsing, avoids some ambiguities.
pub fn string_key_type_guess(key: &str) -> DataModelType {
    match key {
        "cid" => DataModelType::String(LexiconStringType::Cid),
        "uri" => DataModelType::String(LexiconStringType::Uri(UriType::Any)),
        "did" => DataModelType::String(LexiconStringType::Did),
        "handle" => DataModelType::String(LexiconStringType::AtIdentifier),
        "ref" => DataModelType::CidLink,
        "list" => DataModelType::String(LexiconStringType::AtUri),
        "blobref" => DataModelType::Blob,
        "createdAt" | "created" | "indexedAt" | "issuedAt" | "updatedAt" | "playedTime" => {
            DataModelType::String(LexiconStringType::Datetime)
        }
        "size" | "width" | "height" => DataModelType::Integer,
        "value" | "record" | "embed" => DataModelType::Object,
        "text" | "displayName" | "alt" | "name" | "description" => {
            DataModelType::String(LexiconStringType::String)
        }
        "langs" | "blobs" | "images" | "labels" => DataModelType::Array,
        "$bytes" => DataModelType::Bytes,
        "$link" => DataModelType::String(LexiconStringType::Cid),
        "$type" => DataModelType::String(LexiconStringType::String),

        // we assume others are strings speficially because it's easy to check if a serde_json::Value
        // or Ipld value is at least a string, and then we fall back to Object/Map.
        _ => DataModelType::String(LexiconStringType::String),
    }
}

/// Convert an ipld map to a atproto data model blob if it matches the format
pub fn cbor_to_blob<'b>(blob: &'b BTreeMap<String, Ipld>) -> Option<Blob<'b>> {
    let mime_type = blob.get("mimeType").and_then(|o| {
        if let Ipld::String(string) = o {
            Some(string)
        } else {
            None
        }
    });
    if let Some(Ipld::Link(value)) = blob.get("ref") {
        let size = blob.get("size").and_then(|o| {
            if let Ipld::Integer(i) = o {
                Some(*i as i64)
            } else {
                None
            }
        });
        if let (Some(mime_type), Some(size)) = (mime_type, size) {
            return Some(Blob {
                r#ref: Cid::ipld(*value),
                mime_type: MimeType::raw(mime_type),
                size: size as usize,
            });
        }
    } else if let Some(Ipld::String(value)) = blob.get("cid") {
        if let Some(mime_type) = mime_type {
            return Some(Blob {
                r#ref: Cid::str(value),
                mime_type: MimeType::raw(mime_type),
                size: 0,
            });
        }
    }

    None
}

/// convert a JSON object to an atproto data model blob if it matches the format
pub fn json_to_blob<'b>(blob: &'b serde_json::Map<String, serde_json::Value>) -> Option<Blob<'b>> {
    let mime_type = blob.get("mimeType").and_then(|v| v.as_str());
    if let Some(value) = blob.get("ref") {
        if let Some(value) = value
            .as_object()
            .and_then(|o| o.get("$link"))
            .and_then(|v| v.as_str())
        {
            let size = blob.get("size").and_then(|v| v.as_u64());
            if let (Some(mime_type), Some(size)) = (mime_type, size) {
                return Some(Blob {
                    r#ref: Cid::str(value),
                    mime_type: MimeType::raw(mime_type),
                    size: size as usize,
                });
            }
        }
    } else if let Some(value) = blob.get("cid").and_then(|v| v.as_str()) {
        if let Some(mime_type) = mime_type {
            return Some(Blob {
                r#ref: Cid::str(value),
                mime_type: MimeType::raw(mime_type),
                size: 0,
            });
        }
    }

    None
}

/// Infer if something with a "$type" field is a blob or an object
pub fn infer_from_type(type_field: &str) -> DataModelType {
    match type_field {
        "blob" => DataModelType::Blob,
        _ => DataModelType::Object,
    }
}

/// decode a base64 byte string into atproto data
pub fn decode_bytes<'s>(bytes: &str) -> Data<'s> {
    // First one should just work. rest are insurance.
    if let Ok(bytes) = BASE64_STANDARD.decode(bytes) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_STANDARD_NO_PAD.decode(bytes) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_URL_SAFE.decode(bytes) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_URL_SAFE_NO_PAD.decode(bytes) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else {
        Data::String(AtprotoStr::String(CowStr::Borrowed(bytes).into_static()))
    }
}

/// decode a base64 byte string into atproto raw unvalidated data
pub fn decode_raw_bytes<'s>(bytes: &str) -> RawData<'s> {
    // First one should just work. rest are insurance.
    if let Ok(bytes) = BASE64_STANDARD.decode(bytes) {
        RawData::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_STANDARD_NO_PAD.decode(bytes) {
        RawData::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_URL_SAFE.decode(bytes) {
        RawData::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_URL_SAFE_NO_PAD.decode(bytes) {
        RawData::Bytes(Bytes::from_owner(bytes))
    } else {
        RawData::String(CowStr::Borrowed(bytes).into_static())
    }
}
