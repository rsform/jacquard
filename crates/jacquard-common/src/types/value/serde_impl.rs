use core::fmt;
use std::{collections::BTreeMap, str::FromStr};

use base64::{Engine, prelude::BASE64_STANDARD};
use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

use crate::{
    IntoStatic,
    types::{
        DataModelType, LexiconStringType,
        blob::{Blob, MimeType},
        string::*,
        value::{
            Array, AtDataError, Data, Object,
            parsing::{decode_bytes, infer_from_type, parse_string, string_key_type_guess},
        },
    },
};

impl Serialize for Data<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Data::Null => serializer.serialize_none(),
            Data::Boolean(b) => serializer.serialize_bool(*b),
            Data::Integer(i) => serializer.serialize_i64(*i),
            Data::String(s) => s.serialize(serializer),
            Data::Bytes(bytes) => {
                if serializer.is_human_readable() {
                    // JSON: {"$bytes": "base64 string"}
                    use serde::ser::SerializeMap;
                    let mut map = serializer.serialize_map(Some(1))?;
                    map.serialize_entry("$bytes", &BASE64_STANDARD.encode(bytes))?;
                    map.end()
                } else {
                    // CBOR: raw bytes
                    serializer.serialize_bytes(bytes)
                }
            }
            Data::CidLink(cid) => {
                if serializer.is_human_readable() {
                    // JSON: {"$link": "cid_string"}
                    use serde::ser::SerializeMap;
                    let mut map = serializer.serialize_map(Some(1))?;
                    map.serialize_entry("$link", cid.as_str())?;
                    map.end()
                } else {
                    // CBOR: raw cid (Cid's serialize handles this)
                    cid.serialize(serializer)
                }
            }
            Data::Array(arr) => arr.serialize(serializer),
            Data::Object(obj) => obj.serialize(serializer),
            Data::Blob(blob) => blob.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Data<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(DataVisitor)
    }
}

struct DataVisitor;

impl<'de: 'v, 'v> serde::de::Visitor<'v> for DataVisitor {
    type Value = Data<'v>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any valid AT Protocol data value")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Null)
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Boolean(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Integer(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Integer(v as i64))
    }

    fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(E::custom(
            "floating point numbers not allowed in AT protocol data",
        ))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::String(AtprotoStr::String(
            CowStr::Borrowed(v).into_static(),
        )))
    }

    fn visit_borrowed_str<E>(self, v: &'v str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        // Don't infer type here - just store as plain string
        // Type inference happens in apply_type_inference based on field names
        Ok(Data::String(AtprotoStr::String(v.into())))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::String(AtprotoStr::String(v.into())))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Bytes(Bytes::copy_from_slice(v)))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'v>,
    {
        let mut array = Vec::new();
        while let Some(elem) = seq.next_element()? {
            array.push(elem);
        }
        Ok(Data::Array(Array(array)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'v>,
    {
        use serde::de::Error;

        // Peek at first key to check for special single-key patterns
        let mut temp_map: BTreeMap<SmolStr, Data<'v>> = BTreeMap::new();

        while let Some(key) = map.next_key::<SmolStr>()? {
            // Check for special patterns on single-key maps
            if temp_map.is_empty() {
                if key.as_str() == "$link" {
                    // {"$link": "cid_string"} pattern
                    let cid_str: String = map.next_value()?;
                    // Check if there are more keys
                    if let Some(next_key) = map.next_key::<SmolStr>()? {
                        // More keys, treat as regular object
                        temp_map.insert(key, Data::String(AtprotoStr::String(cid_str.into())));
                        let next_value: Data = map.next_value()?;
                        temp_map.insert(next_key, next_value);
                        continue;
                    } else {
                        // Only key, return CidLink
                        return Ok(Data::CidLink(Cid::from(cid_str)));
                    }
                } else if key.as_str() == "$bytes" {
                    // {"$bytes": "base64_string"} pattern
                    let bytes_str: String = map.next_value()?;
                    // Check if there are more keys
                    if map.next_key::<SmolStr>()?.is_some() {
                        // More keys, treat as regular object - shouldn't happen but handle it
                        temp_map.insert(key, Data::String(AtprotoStr::String(bytes_str.into())));
                        continue;
                    } else {
                        // Only key, decode and return bytes
                        return Ok(decode_bytes(&bytes_str));
                    }
                }
            }

            let value: Data = map.next_value()?;
            temp_map.insert(key, value);
        }

        // Second pass: apply type inference and check for special patterns
        apply_type_inference(temp_map).map_err(A::Error::custom)
    }
}

fn apply_type_inference<'s>(mut map: BTreeMap<SmolStr, Data<'s>>) -> Result<Data<'s>, AtDataError> {
    // Check for CID link pattern first: {"$link": "cid_string"}
    if map.len() == 1 {
        if let Some(Data::String(AtprotoStr::String(link))) = map.get("$link") {
            // Need to extract ownership, can't borrow from map we're about to consume
            let link_owned = link.clone();
            return Ok(Data::CidLink(Cid::cow_str(link_owned)));
        }
    }

    // Check for $type field to detect special structures
    let type_field = map.get("$type").and_then(|v| {
        if let Data::String(AtprotoStr::String(s)) = v {
            Some(s.as_ref())
        } else {
            None
        }
    });

    // Check for blob
    if let Some(type_str) = type_field {
        if type_str == "blob" && infer_from_type(type_str) == DataModelType::Blob {
            // Try to construct blob from the collected data
            let ref_cid = map.get("ref").and_then(|v| {
                if let Data::CidLink(cid) = v {
                    Some(cid.clone())
                } else {
                    None
                }
            });

            let mime_type = map.get("mimeType").and_then(|v| {
                if let Data::String(AtprotoStr::String(s)) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });

            let size = map.get("size").and_then(|v| {
                if let Data::Integer(i) = v {
                    Some(*i as usize)
                } else {
                    None
                }
            });

            if let (Some(ref_cid), Some(mime_cowstr), Some(size)) = (ref_cid, mime_type, size) {
                return Ok(Data::Blob(Blob {
                    r#ref: ref_cid,
                    mime_type: MimeType::from(mime_cowstr),
                    size,
                }));
            }
        }
    }

    // Apply type inference for string fields based on key names (mutate in place)
    for (key, value) in map.iter_mut() {
        if let Data::String(AtprotoStr::String(s)) = value.to_owned() {
            let type_hint = string_key_type_guess(key.as_str());
            let refined = match type_hint {
                DataModelType::String(string_type) => refine_string_by_type(s, string_type),
                DataModelType::Bytes => {
                    // Decode base64
                    decode_bytes(&s)
                }
                DataModelType::CidLink if key.as_str() == "$link" => {
                    Data::CidLink(Cid::from_str(&s).unwrap())
                }
                _ => continue, // no refinement needed
            };
            *value = refined;
        }
    }

    Ok(Data::Object(Object(map)))
}

fn refine_string_by_type<'s>(s: CowStr<'s>, string_type: LexiconStringType) -> Data<'s> {
    match string_type {
        LexiconStringType::Datetime => Datetime::from_str(&s)
            .map(|dt| Data::String(AtprotoStr::Datetime(dt)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::AtUri => AtUri::new_owned(s.clone())
            .map(|uri| Data::String(AtprotoStr::AtUri(uri)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::Did => Did::new_owned(s.clone())
            .map(|did| Data::String(AtprotoStr::Did(did)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::Handle => Handle::new_owned(s.clone())
            .map(|handle| Data::String(AtprotoStr::Handle(handle)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::AtIdentifier => AtIdentifier::new_owned(s.clone())
            .map(|ident| Data::String(AtprotoStr::AtIdentifier(ident)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::Nsid => Nsid::new_owned(s.clone())
            .map(|nsid| Data::String(AtprotoStr::Nsid(nsid)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::Cid => Cid::new_owned(s.as_bytes())
            .map(|cid| Data::String(AtprotoStr::Cid(cid)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.into()))),
        LexiconStringType::Language => Language::new(&s)
            .map(|lang| Data::String(AtprotoStr::Language(lang)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::Tid => Tid::new(s.clone())
            .map(|tid| Data::String(AtprotoStr::Tid(tid)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::RecordKey => Rkey::new_owned(s.clone())
            .map(|rkey| Data::String(AtprotoStr::RecordKey(RecordKey::from(rkey))))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::Uri(_) => Uri::new_owned(s.clone())
            .map(|uri| Data::String(AtprotoStr::Uri(uri)))
            .unwrap_or_else(|_| Data::String(AtprotoStr::String(s.clone()))),
        LexiconStringType::String => Data::String(parse_string(&s).into_static()),
    }
}

impl Serialize for Array<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for item in &self.0 {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Array<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Just deserialize as Vec<Data> directly - the Data visitor handles everything
        let vec: Vec<Data<'de>> = Deserialize::deserialize(deserializer)?;
        Ok(Array(vec))
    }
}

impl Serialize for Object<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key.as_str(), value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Object<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        // Deserialize via Data, then extract the Object
        // The Data visitor handles all the type inference and special cases
        let data: Data<'de> = Data::deserialize(deserializer)?;
        match data {
            Data::Object(obj) => Ok(obj),
            _ => Err(D::Error::custom("expected object, got something else")),
        }
    }
}
