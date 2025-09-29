use base64::{
    Engine,
    prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD, BASE64_URL_SAFE, BASE64_URL_SAFE_NO_PAD},
};
use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::{SmolStr, ToSmolStr};
use std::{collections::BTreeMap, str::FromStr};
use url::Url;

use crate::types::{
    DataModelType, LexiconStringType,
    blob::{Blob, MimeType},
    string::*,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data<'s> {
    Null,
    Boolean(bool),
    Integer(i64),
    String(AtprotoStr<'s>),
    Bytes(Bytes),     // maybe need custom type for serialization
    CidLink(Cid<'s>), // maybe need custom type for serialization
    Array(Array<'s>),
    Object(Object<'s>),
    Blob(Blob<'s>),
}

impl<'s> Data<'s> {
    pub fn from_json(json: &'s serde_json::Value) -> Self {
        if let Some(value) = json.as_bool() {
            Self::Boolean(value)
        } else if let Some(value) = json.as_i64() {
            Self::Integer(value)
        } else if let Some(value) = json.as_str() {
            Self::String(AtprotoStr::new(value))
        } else if let Some(value) = json.as_array() {
            Self::Array(Array::from_json(value))
        } else if let Some(value) = json.as_object() {
            Object::from_json(value)
        } else if let Some(num) = json.as_number() {
            // deliberately permissive here, just in case.
            Self::String(AtprotoStr::new_owned(num.to_smolstr()))
        } else {
            Self::Null
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array<'s>(pub Vec<Data<'s>>);

impl<'s> Array<'s> {
    pub fn from_json(json: &'s Vec<serde_json::Value>) -> Self {
        let mut array = Vec::with_capacity(json.len());
        for item in json {
            array.push(Data::from_json(item));
        }
        Self(array)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<'s>(pub BTreeMap<SmolStr, Data<'s>>);

impl<'s> Object<'s> {
    pub fn from_json(json: &'s serde_json::Map<String, serde_json::Value>) -> Data<'s> {
        if let Some(type_field) = json.get("$type").and_then(|v| v.as_str()) {
            if infer_from_type(type_field) == DataModelType::Blob {
                if let Some(blob) = json_to_blob(json) {
                    return Data::Blob(blob);
                }
            }
        }
        let mut map = BTreeMap::new();

        for (key, value) in json {
            if key == "$type" {
                continue; // skip, because we've already handled it
            }
            match string_key_type_guess(key) {
                DataModelType::Null => {
                    if value.is_null() {
                        map.insert(key.to_smolstr(), Data::Null);
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::Boolean => {
                    if let Some(value) = value.as_bool() {
                        map.insert(key.to_smolstr(), Data::Boolean(value));
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::Integer => {
                    if let Some(int) = value.as_i64() {
                        map.insert(key.to_smolstr(), Data::Integer(int));
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::Bytes => {
                    if let Some(value) = value.as_str() {
                        map.insert(key.to_smolstr(), decode_bytes(value));
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::CidLink => {
                    if let Some(value) = value.as_str() {
                        map.insert(
                            key.to_smolstr(),
                            Data::String(AtprotoStr::Cid(Cid::Str(value.into()))),
                        );
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::Blob => {
                    if let Some(value) = value.as_object() {
                        map.insert(key.to_smolstr(), Object::from_json(value));
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::Array => {
                    if let Some(value) = value.as_array() {
                        map.insert(key.to_smolstr(), Data::Array(Array::from_json(value)));
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::Object => {
                    if let Some(value) = value.as_object() {
                        map.insert(key.to_smolstr(), Object::from_json(value));
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
                DataModelType::String(string_type) => {
                    if let Some(value) = value.as_str() {
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
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::AtUri(value)),
                                    );
                                } else {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::String(value.into())),
                                    );
                                }
                            }
                            LexiconStringType::Did => {
                                if let Ok(value) = Did::new(value) {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Did(value)),
                                    );
                                } else {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::String(value.into())),
                                    );
                                }
                            }
                            LexiconStringType::Handle => {
                                if let Ok(value) = Handle::new(value) {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Handle(value)),
                                    );
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
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Nsid(value)),
                                    );
                                } else {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::String(value.into())),
                                    );
                                }
                            }
                            LexiconStringType::Cid => {
                                if let Ok(value) = Cid::new(value.as_bytes()) {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Cid(value)),
                                    );
                                } else {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::String(value.into())),
                                    );
                                }
                            }
                            LexiconStringType::Language => {
                                if let Ok(value) = Language::new(value) {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Language(value)),
                                    );
                                } else {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::String(value.into())),
                                    );
                                }
                            }
                            LexiconStringType::Tid => {
                                if let Ok(value) = Tid::new(value) {
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Tid(value)),
                                    );
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
                                    map.insert(
                                        key.to_smolstr(),
                                        Data::String(AtprotoStr::Uri(uri)),
                                    );
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
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value));
                    }
                }
            }
        }

        Data::Object(Object(map))
    }

    //pub fn from_cbor(cbor: BTreeMap<String, ipld_core::ipld::Ipld>) -> Self {}
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
    } else if !string.contains([' ', '\n']) {
        // cid?
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
        "uri" => DataModelType::String(LexiconStringType::Uri(super::UriType::Any)),
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

pub fn infer_from_type(type_field: &str) -> DataModelType {
    match type_field {
        "blob" => DataModelType::Blob,
        _ => DataModelType::Object,
    }
}

pub fn decode_bytes<'s>(bytes: &'s str) -> Data<'s> {
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
        Data::String(AtprotoStr::String(bytes.into()))
    }
}
