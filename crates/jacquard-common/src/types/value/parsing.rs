use crate::Bos;
use crate::deps::fluent_uri::Uri;
use crate::types::aturi::validate_and_index;
use crate::types::cid::IpldCid;
use crate::types::did::validate_did;
use crate::types::handle::validate_handle;
use crate::types::nsid::validate_nsid;
use crate::{
    IntoStatic,
    types::{
        DataModelType, LexiconStringType, UriType,
        blob::{Blob, MimeType},
        string::*,
        value::{AtDataError, Data, RawData},
    },
};
use alloc::collections::BTreeMap;
use alloc::string::String;
use base64::{
    Engine,
    prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD, BASE64_URL_SAFE, BASE64_URL_SAFE_NO_PAD},
};
use bytes::Bytes;
use core::str::FromStr;
use ipld_core::ipld::Ipld;
use smol_str::{SmolStr, ToSmolStr};

/// Insert a string into an at:// `Data<'_>` map, inferring its type.
pub fn insert_string<'s, S>(
    map: &mut BTreeMap<SmolStr, Data<S>>,
    key: &'s str,
    value: S,
    string_type: LexiconStringType,
) -> Result<(), AtDataError>
where
    S: AsRef<str> + Bos<str>,
{
    match string_type {
        LexiconStringType::Datetime => {
            if let Ok(datetime) = Datetime::from_str(value.as_ref()) {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::Datetime(datetime)),
                );
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::AtUri => {
            if validate_and_index(value.as_ref()).is_ok() {
                // AtprotoStr::AtUri stores AtUri<'static>; convert to owned.
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::AtUri(unsafe { AtUri::unchecked(value) })),
                );
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::Did => {
            if validate_did(value.as_ref()).is_ok() {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::Did(unsafe { Did::unchecked(value) })),
                );
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::Handle => {
            if validate_handle(value.as_ref()).is_ok() {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::Handle(unsafe { Handle::unchecked(value) })),
                );
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::AtIdentifier => {
            if validate_handle(value.as_ref()).is_ok() || validate_did(value.as_ref()).is_ok() {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::AtIdentifier(unsafe {
                        AtIdentifier::unchecked(value)
                    })),
                );
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Nsid => {
            if validate_nsid(value.as_ref()).is_ok() {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::Nsid(unsafe { Nsid::unchecked(value) })),
                );
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::Cid => {
            let s: &str = value.as_ref();
            // CID: try to parse as IPLD first, otherwise wrap as string CID.
            if IpldCid::try_from(s).is_ok() || s.starts_with("bafy") {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::Cid(unsafe { Cid::unchecked_str(value) })),
                );
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Language => {
            if let Ok(value) = Language::new(value.as_ref()) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Language(value)));
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::Tid => {
            if let Ok(value) = Tid::new(value.as_ref()) {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::Tid(value)));
            } else {
                map.insert(key.to_smolstr(), Data::String(AtprotoStr::String(value)));
            }
        }
        LexiconStringType::RecordKey => {
            // Validate the rkey without shadowing the original `value: &'s str`.
            if Rkey::new(value.as_ref()).is_ok() {
                map.insert(
                    key.to_smolstr(),
                    // Rkey already validated above; borrow the original &'s str directly.
                    Data::String(AtprotoStr::RecordKey(
                        RecordKey::any(value).expect("Rkey validation passed"),
                    )),
                );
            } else {
                map.insert(
                    key.to_smolstr(),
                    Data::String(AtprotoStr::String(value.into())),
                );
            }
        }
        LexiconStringType::Uri(_) => {
            // AtprotoStr::Uri stores UriValue<'static>, so we must produce an owned value.
            if let Ok(uri) = UriValue::new(value.as_ref()) {
                match uri {
                    UriValue::Any(_) => {
                        map.insert(
                            key.to_smolstr(),
                            Data::String(AtprotoStr::String(value.into())),
                        );
                    }
                    _ => {
                        map.insert(
                            key.to_smolstr(),
                            Data::String(AtprotoStr::Uri(
                                UriValue::new(value).expect("already verified"),
                            )),
                        );
                    }
                }
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
pub fn parse_string<S>(string: S) -> AtprotoStr<S>
where
    S: Bos<str> + AsRef<str>,
{
    let s = string.as_ref();
    if s.len() < 2048 && s.starts_with("did:") {
        if validate_did(s).is_ok() {
            return AtprotoStr::Did(unsafe { Did::unchecked(string) });
        }
    } else if s.starts_with("20") && s.ends_with("Z") {
        // probably a date (for the next 75 years)
        if let Ok(datetime) = Datetime::from_str(s) {
            return AtprotoStr::Datetime(datetime);
        }
    } else if s.starts_with("at://") {
        if crate::types::aturi::validate_and_index(s).is_ok() {
            return AtprotoStr::AtUri(unsafe { AtUri::unchecked(string) });
        }
    } else if s.starts_with("https://") {
        if let Ok(uri) = Uri::parse(s) {
            return AtprotoStr::Uri(UriValue::Https(uri.to_owned()));
        }
    } else if s.starts_with("wss://") {
        if let Ok(uri) = Uri::parse(s) {
            return AtprotoStr::Uri(UriValue::Wss(uri.to_owned()));
        }
    } else if s.starts_with("ipfs://") {
        let s: &str = string.as_ref();
        // CID: try to parse as IPLD first, otherwise wrap as string CID.
        if IpldCid::try_from(s).is_ok() || s.starts_with("bafy") {
            return AtprotoStr::Uri(UriValue::Cid(unsafe { Cid::unchecked_str(string) }));
        }
    } else if s.contains('.') && !s.contains([' ', '\n']) {
        // Dotted strings without a scheme could be handles, NSIDs, or URIs.
        // Use TLD lookup and camelCase heuristic to disambiguate.
        // - Handles: domain order with TLD at the end (e.g., "example.com")
        // - NSIDs: reverse domain order with TLD at the start (e.g., "com.example.service")
        // - Tiebreaker: camelCase in the last segment indicates NSID (e.g., "getRecord")
        let first_segment = s.split('.').next().unwrap_or("");
        let last_segment = s.rsplit('.').next().unwrap_or("");

        let first_is_tld = crate::tld::is_tld(first_segment);
        let last_is_tld = crate::tld::is_tld(last_segment);
        let has_upper_last_segment = last_segment.chars().any(|c| c.is_ascii_uppercase());

        // First segment is a known TLD → reverse domain order → try NSID first.
        if first_is_tld {
            if validate_nsid(s).is_ok() {
                return AtprotoStr::Nsid(unsafe { Nsid::unchecked(string) });
            }
        }

        // Last segment is a known TLD and first is not → normal domain order → handle.
        if last_is_tld && !first_is_tld {
            if validate_handle(s.as_ref()).is_ok() || validate_did(s.as_ref()).is_ok() {
                return AtprotoStr::AtIdentifier(unsafe { AtIdentifier::unchecked(string) });
            }
        }

        // camelCase in last segment → NSID (e.g., "com.atproto.repo.getRecord").
        if has_upper_last_segment {
            if validate_nsid(s).is_ok() {
                return AtprotoStr::Nsid(unsafe { Nsid::unchecked(string) });
            }
        }

        // Fallback: try both, preferring handle.
        if validate_handle(s.as_ref()).is_ok() {
            return AtprotoStr::AtIdentifier(AtIdentifier::Handle(unsafe {
                Handle::unchecked(string)
            }));
        } else if validate_nsid(s).is_ok() {
            return AtprotoStr::Nsid(unsafe { Nsid::unchecked(string) });
        } else if s.contains("://") && Uri::<&str>::parse(s).is_ok() {
            // AtprotoStr::Uri stores UriValue<'static>; convert to owned.
            return AtprotoStr::Uri(UriValue::Any(string));
        }
    } else if s.len() == 13 {
        if let Ok(tid) = Tid::new(string.as_ref()) {
            return AtprotoStr::Tid(tid);
        }
    } else if !s.contains([' ', '\n']) && s.len() > 20 {
        // CID: try to parse as IPLD first, otherwise wrap as string CID.
        if IpldCid::try_from(s).is_ok() || s.starts_with("bafy") {
            return AtprotoStr::Cid(unsafe { Cid::unchecked_str(string) });
        }
    }

    AtprotoStr::String(string)
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
pub fn cbor_to_blob<'b>(blob: &'b BTreeMap<String, Ipld>) -> Option<Blob<CowStr<'b>>> {
    let mime_type = blob.get("mimeType").and_then(|o| {
        if let Ipld::String(string) = o {
            Some(string.as_str())
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
                r#ref: CidLink::<CowStr<'b>>::ipld(*value),
                mime_type: MimeType::new(CowStr::Borrowed(mime_type)),
                size: size as usize,
            });
        }
    } else if let Some(Ipld::String(value)) = blob.get("cid") {
        if let Some(mime_type) = mime_type {
            return Some(Blob {
                r#ref: CidLink::cow_str(CowStr::Borrowed(value.as_str())),
                mime_type: MimeType::new(CowStr::Borrowed(mime_type)),
                size: 0,
            });
        }
    }

    None
}

/// convert a JSON object to an atproto data model blob if it matches the format
pub fn json_to_blob<'b>(
    blob: &'b serde_json::Map<String, serde_json::Value>,
) -> Option<Blob<CowStr<'b>>> {
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
                    r#ref: CidLink::cow_str(CowStr::Borrowed(value)),
                    mime_type: MimeType::new(CowStr::Borrowed(mime_type)),
                    size: size as usize,
                });
            }
        }
    } else if let Some(value) = blob.get("cid").and_then(|v| v.as_str()) {
        if let Some(mime_type) = mime_type {
            return Some(Blob {
                r#ref: CidLink::cow_str(CowStr::Borrowed(value)),
                mime_type: MimeType::new(CowStr::Borrowed(mime_type)),
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
pub fn decode_bytes<S>(bytes: S) -> Data<S>
where
    S: Bos<str> + AsRef<str>,
{
    // First one should just work. rest are insurance.
    if let Ok(bytes) = BASE64_STANDARD.decode(bytes.as_ref().as_bytes()) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_STANDARD_NO_PAD.decode(bytes.as_ref().as_bytes()) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_URL_SAFE.decode(bytes.as_ref().as_bytes()) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else if let Ok(bytes) = BASE64_URL_SAFE_NO_PAD.decode(bytes.as_ref().as_bytes()) {
        Data::Bytes(Bytes::from_owner(bytes))
    } else {
        Data::String(AtprotoStr::String(S::from(bytes)))
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
