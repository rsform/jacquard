use core::fmt;
use std::{collections::BTreeMap, str::FromStr};

use base64::{Engine, prelude::BASE64_STANDARD};
use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::VariantAccess};
use smol_str::{SmolStr, ToSmolStr};

use crate::{
    IntoStatic,
    types::{
        DataModelType, LexiconStringType,
        blob::{Blob, MimeType},
        string::*,
        value::{
            Array, AtDataError, Data, Object, RawData,
            parsing::{
                decode_bytes, decode_raw_bytes, infer_from_type, parse_string,
                string_key_type_guess,
            },
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

impl<'de, 'a> Deserialize<'de> for Data<'a>
where
    'de: 'a,
{
    /// Currently only works for self-describing formats
    /// Thankfully the supported atproto data formats are both self-describing (json and dag-cbor).
    /// TODO: see if there's any way to make this work with Postcard.
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

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'v>,
    {
        Ok(deserializer.deserialize_any(self)?)
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
        Ok(Data::Integer((v % (i64::MAX as u64)) as i64))
    }

    fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::String(AtprotoStr::String(
            CowStr::Owned(_v.to_smolstr()).into_static(),
        )))
        // Err(E::custom(
        //     "floating point numbers not allowed in AT protocol data",
        // ))
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

    fn visit_borrowed_bytes<E>(self, v: &'v [u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Bytes(Bytes::copy_from_slice(v)))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Data::Bytes(Bytes::from_owner(v)))
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::EnumAccess<'v>,
    {
        match data.variant::<SmolStr>() {
            Ok((key, value)) => {
                let mut map = BTreeMap::new();
                if let Ok(variant) = value.newtype_variant::<Data>() {
                    map.insert(key, variant);
                }
                Ok(Data::Object(Object(map)))
            }
            Err(e) => Err(e),
        }
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

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'v>,
    {
        deserializer.deserialize_map(self)
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
        if infer_from_type(type_str) == DataModelType::Blob {
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

impl<'de, 'a> Deserialize<'de> for Array<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Just deserialize as Vec<Data> directly - the Data visitor handles everything
        let vec: Vec<Data<'a>> = Deserialize::deserialize(deserializer)?;
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

impl<'de, 'a> Deserialize<'de> for Object<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        // Deserialize via Data, then extract the Object
        // The Data visitor handles all the type inference and special cases
        let data: Data<'a> = Data::deserialize(deserializer)?;
        match data {
            Data::Object(obj) => Ok(obj),
            _ => Err(D::Error::custom("expected object, got something else")),
        }
    }
}

impl Serialize for RawData<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            RawData::Null => serializer.serialize_none(),
            RawData::Boolean(b) => serializer.serialize_bool(*b),
            RawData::SignedInt(i) => serializer.serialize_i64(*i),
            RawData::UnsignedInt(u) => serializer.serialize_u64(*u),
            RawData::String(s) => serializer.serialize_str(&s),
            RawData::Bytes(bytes) => {
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
            RawData::CidLink(cid) => {
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
            RawData::Array(arr) => arr.serialize(serializer),
            RawData::Object(obj) => obj.serialize(serializer),
            RawData::Blob(blob) => blob.serialize(serializer),
            RawData::InvalidBlob(raw_data) => raw_data.serialize(serializer),
            RawData::InvalidNumber(bytes) => serializer.serialize_bytes(bytes),
            RawData::InvalidData(bytes) => serializer.serialize_bytes(bytes),
        }
    }
}

impl<'de, 'a> Deserialize<'de> for RawData<'a>
where
    'de: 'a,
{
    /// Currently only works for self-describing formats
    /// Thankfully the supported atproto data formats are both self-describing (json and dag-cbor).
    /// TODO: see if there's any way to make this work with Postcard.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RawDataVisitor)
    }
}

struct RawDataVisitor;

impl<'de: 'v, 'v> serde::de::Visitor<'v> for RawDataVisitor {
    type Value = RawData<'v>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any valid AT Protocol data value")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'v>,
    {
        Ok(deserializer.deserialize_option(self)?)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::Null)
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::Boolean(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::SignedInt(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::UnsignedInt(v))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::InvalidNumber(Bytes::from_owner(v.to_be_bytes())))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::String(CowStr::Borrowed(v).into_static()))
    }

    fn visit_borrowed_str<E>(self, v: &'v str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::String(v.into()))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::String(v.into()))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::Bytes(Bytes::copy_from_slice(v)))
    }

    fn visit_borrowed_bytes<E>(self, v: &'v [u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::Bytes(Bytes::copy_from_slice(v)))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RawData::Bytes(Bytes::from_owner(v)))
    }

    // check on this, feels weird
    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::EnumAccess<'v>,
    {
        match data.variant::<SmolStr>() {
            Ok((key, value)) => {
                let mut map = BTreeMap::new();
                if let Ok(variant) = value.newtype_variant::<RawData>() {
                    map.insert(key, variant);
                }
                Ok(RawData::Object(map))
            }
            Err(e) => Err(e),
        }
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'v>,
    {
        let mut array = Vec::new();
        while let Some(elem) = seq.next_element()? {
            array.push(elem);
        }
        Ok(RawData::Array(array))
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'v>,
    {
        deserializer.deserialize_map(self)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'v>,
    {
        use serde::de::Error;

        // Peek at first key to check for special single-key patterns
        let mut temp_map: BTreeMap<SmolStr, RawData<'v>> = BTreeMap::new();

        while let Some(key) = map.next_key::<SmolStr>()? {
            // Check for special patterns on single-key maps
            if temp_map.is_empty() {
                if key.as_str() == "$link" {
                    // {"$link": "cid_string"} pattern
                    let cid_str: String = map.next_value()?;
                    // Check if there are more keys
                    if let Some(next_key) = map.next_key::<SmolStr>()? {
                        // More keys, treat as regular object
                        temp_map.insert(key, RawData::String(cid_str.into()));
                        let next_value: RawData = map.next_value()?;
                        temp_map.insert(next_key, next_value);
                        continue;
                    } else {
                        // Only key, return CidLink
                        return Ok(RawData::CidLink(Cid::from(cid_str)));
                    }
                } else if key.as_str() == "$bytes" {
                    // {"$bytes": "base64_string"} pattern
                    let bytes_str: String = map.next_value()?;
                    // Check if there are more keys
                    if map.next_key::<SmolStr>()?.is_some() {
                        // More keys, treat as regular object - shouldn't happen but handle it
                        temp_map.insert(key, RawData::String(bytes_str.into()));
                        continue;
                    } else {
                        // Only key, decode and return bytes
                        return Ok(decode_raw_bytes(&bytes_str));
                    }
                }
            }

            let value: RawData = map.next_value()?;
            temp_map.insert(key, value);
        }

        // Second pass: apply type inference and check for special patterns
        apply_raw_type_inference(temp_map).map_err(A::Error::custom)
    }
}

fn apply_raw_type_inference<'s>(
    map: BTreeMap<SmolStr, RawData<'s>>,
) -> Result<RawData<'s>, AtDataError> {
    // Check for CID link pattern first: {"$link": "cid_string"}
    if map.len() == 1 {
        if let Some(RawData::String(link)) = map.get("$link") {
            // Need to extract ownership, can't borrow from map we're about to consume
            let link_owned = link.clone();
            return Ok(RawData::CidLink(Cid::cow_str(link_owned)));
        }
    }

    // Check for $type field to detect special structures
    let type_field = map.get("$type").and_then(|v| {
        if let RawData::String(s) = v {
            Some(s.as_ref())
        } else {
            None
        }
    });

    // Check for blob
    if let Some(type_str) = type_field {
        if infer_from_type(type_str) == DataModelType::Blob {
            // Try to construct blob from the collected data
            let ref_cid = map.get("ref").and_then(|v| {
                if let RawData::CidLink(cid) = v {
                    Some(cid.clone())
                } else {
                    None
                }
            });

            let mime_type = map.get("mimeType").and_then(|v| {
                if let RawData::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });

            let size = map.get("size").and_then(|v| {
                if let RawData::UnsignedInt(i) = v {
                    Some(*i as usize)
                } else if let RawData::SignedInt(i) = v {
                    Some(*i as usize)
                } else {
                    None
                }
            });

            if let (Some(ref_cid), Some(mime_cowstr), Some(size)) = (ref_cid, mime_type, size) {
                return Ok(RawData::Blob(Blob {
                    r#ref: ref_cid,
                    mime_type: MimeType::from(mime_cowstr),
                    size,
                }));
            } else {
                return Ok(RawData::InvalidBlob(Box::new(RawData::Object(map))));
            }
        }
    }

    Ok(RawData::Object(map))
}

// Deserializer implementation for &Data<'de> - allows deserializing typed data from Data values
impl<'de> serde::Deserializer<'de> for &'de Data<'de> {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Data::Null => visitor.visit_unit(),
            Data::Boolean(b) => visitor.visit_bool(*b),
            Data::Integer(i) => visitor.visit_i64(*i),
            Data::String(s) => {
                // Get the string with 'de lifetime first
                let string_ref: &'de str = s.as_str();

                // Try to borrow from types that contain CowStr
                match s {
                    AtprotoStr::String(cow) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::Did(Did(cow)) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::Handle(Handle(cow)) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::Nsid(Nsid(cow)) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::Uri(Uri::Did(Did(cow))) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::Uri(Uri::Any(cow)) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::Cid(Cid::Str(cow)) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::AtIdentifier(AtIdentifier::Did(Did(cow))) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::AtIdentifier(AtIdentifier::Handle(Handle(cow))) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    AtprotoStr::RecordKey(RecordKey(Rkey(cow))) => match cow {
                        CowStr::Borrowed(b) => visitor.visit_borrowed_str(b),
                        CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
                    },
                    // All other types (Tid, Datetime, Language, AtUri with SmolStr):
                    // use visit_borrowed_str with the &'de str so they can borrow if needed
                    _ => visitor.visit_borrowed_str(string_ref),
                }
            }
            Data::Bytes(b) => visitor.visit_bytes(b),
            Data::CidLink(cid) => visitor.visit_str(cid.as_str()),
            Data::Array(arr) => visitor.visit_seq(ArrayDeserializer::new(&arr.0)),
            Data::Object(obj) => visitor.visit_map(ObjectDeserializer::new(&obj.0)),
            Data::Blob(blob) => {
                // Blob is a root type - deserialize as the Blob itself via map representation
                visitor.visit_map(BlobDeserializer::new(blob))
            }
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// Deserializer implementation for &Data<'de> - allows deserializing typed data from Data values
impl<'de> serde::Deserializer<'de> for Data<'static> {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Data::Null => visitor.visit_unit(),
            Data::Boolean(b) => visitor.visit_bool(b),
            Data::Integer(i) => visitor.visit_i64(i),
            Data::String(s) => visitor.visit_str(s.as_str()),
            Data::Bytes(b) => visitor.visit_bytes(b.as_ref()),
            Data::CidLink(cid) => visitor.visit_str(cid.as_str()),
            Data::Array(arr) => visitor.visit_seq(OwnedArrayDeserializer::new(arr.0)),
            Data::Object(obj) => visitor.visit_map(OwnedObjectDeserializer::new(obj.0)),
            Data::Blob(blob) => {
                // Blob is a root type - deserialize as the Blob itself via map representation
                visitor.visit_map(OwnedBlobDeserializer::new(blob))
            }
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// Deserializer implementation for &RawData<'de>
impl<'de> serde::Deserializer<'de> for &'de RawData<'de> {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            RawData::Null => visitor.visit_unit(),
            RawData::Boolean(b) => visitor.visit_bool(*b),
            RawData::SignedInt(i) => visitor.visit_i64(*i),
            RawData::UnsignedInt(u) => visitor.visit_u64(*u),
            RawData::String(cow) => match cow {
                CowStr::Borrowed(s) => visitor.visit_borrowed_str(s),
                CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
            },
            RawData::Bytes(b) => visitor.visit_bytes(b),
            RawData::CidLink(cid) => visitor.visit_str(cid.as_str()),
            RawData::Array(arr) => visitor.visit_seq(RawArrayDeserializer::new(arr)),
            RawData::Object(obj) => visitor.visit_map(RawObjectDeserializer::new(obj)),
            RawData::Blob(blob) => visitor.visit_map(BlobDeserializer::new(blob)),
            RawData::InvalidBlob(data) => data.as_ref().deserialize_any(visitor),
            RawData::InvalidNumber(bytes) => visitor.visit_bytes(bytes),
            RawData::InvalidData(bytes) => visitor.visit_bytes(bytes),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// Deserializer implementation for &RawData<'de>
impl<'de> serde::Deserializer<'de> for RawData<'static> {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            RawData::Null => visitor.visit_unit(),
            RawData::Boolean(b) => visitor.visit_bool(b),
            RawData::SignedInt(i) => visitor.visit_i64(i),
            RawData::UnsignedInt(u) => visitor.visit_u64(u),
            RawData::String(cow) => match cow {
                CowStr::Borrowed(s) => visitor.visit_borrowed_str(s),
                CowStr::Owned(_) => visitor.visit_str(cow.as_ref()),
            },
            RawData::Bytes(b) => visitor.visit_bytes(b.as_ref()),
            RawData::CidLink(cid) => visitor.visit_str(cid.as_str()),
            RawData::Array(arr) => visitor.visit_seq(RawOwnedArrayDeserializer::new(arr)),
            RawData::Object(obj) => visitor.visit_map(RawOwnedObjectDeserializer::new(obj)),
            RawData::Blob(blob) => visitor.visit_map(OwnedBlobDeserializer::new(blob)),
            RawData::InvalidBlob(data) => data.deserialize_any(visitor),
            RawData::InvalidNumber(bytes) => visitor.visit_bytes(bytes.as_ref()),
            RawData::InvalidData(bytes) => visitor.visit_bytes(bytes.as_ref()),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

/// Error type for Data/RawData deserializer
#[derive(Debug, Clone, thiserror::Error)]
pub enum DataDeserializerError {
    /// Custom error message
    #[error("{0}")]
    Message(String),
    /// Invalid type error
    #[error("invalid type: expected {expected}, found {found}")]
    InvalidType {
        /// Expected type
        expected: String,
        /// Found type
        found: String,
    },
    /// Unknown field error
    #[error("unknown field: {0}")]
    UnknownField(String),
    /// Missing field error
    #[error("missing field: {0}")]
    MissingField(String),
}

impl serde::de::Error for DataDeserializerError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DataDeserializerError::Message(msg.to_string())
    }
}

// MapAccess implementation for Blob - allows borrowing from blob fields
struct BlobDeserializer<'de> {
    blob: &'de Blob<'de>,
    field_index: usize,
}

impl<'de> BlobDeserializer<'de> {
    fn new(blob: &'de Blob<'de>) -> Self {
        Self {
            blob,
            field_index: 0,
        }
    }
}

impl<'de> serde::de::MapAccess<'de> for BlobDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        let key = match self.field_index {
            0 => "$type",
            1 => "ref",
            2 => "mimeType",
            3 => "size",
            _ => return Ok(None),
        };
        self.field_index += 1;
        seed.deserialize(BorrowedStrDeserializer(key)).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.field_index - 1 {
            0 => seed.deserialize(BorrowedStrDeserializer("blob")),
            1 => seed.deserialize(BorrowedStrDeserializer(self.blob.r#ref.as_str())),
            2 => seed.deserialize(BorrowedStrDeserializer(self.blob.mime_type.as_str())),
            3 => seed.deserialize(I64Deserializer(self.blob.size as i64)),
            _ => Err(DataDeserializerError::Message(
                "invalid field index".to_string(),
            )),
        }
    }
}

struct OwnedBlobDeserializer {
    blob: Blob<'static>,
    field_index: usize,
}

impl OwnedBlobDeserializer {
    fn new(blob: Blob<'_>) -> Self {
        Self {
            blob: blob.into_static(),
            field_index: 0,
        }
    }
}

impl<'de> serde::de::MapAccess<'de> for OwnedBlobDeserializer {
    type Error = DataDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        let key = match self.field_index {
            0 => "$type",
            1 => "ref",
            2 => "mimeType",
            3 => "size",
            _ => return Ok(None),
        };
        self.field_index += 1;
        seed.deserialize(BorrowedStrDeserializer(key)).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.field_index - 1 {
            0 => seed.deserialize(OwnedStrDeserializer("blob".into())),
            1 => seed.deserialize(OwnedStrDeserializer(self.blob.r#ref.to_smolstr())),
            2 => seed.deserialize(OwnedStrDeserializer(self.blob.mime_type.to_smolstr())),
            3 => seed.deserialize(I64Deserializer(self.blob.size as i64)),
            _ => Err(DataDeserializerError::Message(
                "invalid field index".to_string(),
            )),
        }
    }
}

// Helper deserializer for borrowed strings
struct BorrowedStrDeserializer<'de>(&'de str);

impl<'de> serde::Deserializer<'de> for BorrowedStrDeserializer<'de> {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.0)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// Helper deserializer for borrowed strings
struct OwnedStrDeserializer(SmolStr);

impl<'de> serde::Deserializer<'de> for OwnedStrDeserializer {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_str(&self.0)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// Helper deserializer for i64 values
struct I64Deserializer(i64);

impl<'de> serde::Deserializer<'de> for I64Deserializer {
    type Error = DataDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i64(self.0)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// SeqAccess implementation for Data::Array
struct ArrayDeserializer<'de> {
    iter: std::slice::Iter<'de, Data<'de>>,
}

impl<'de> ArrayDeserializer<'de> {
    fn new(slice: &'de [Data<'de>]) -> Self {
        Self { iter: slice.iter() }
    }
}

impl<'de> serde::de::SeqAccess<'de> for ArrayDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }
}

// SeqAccess implementation for Data::Array
struct OwnedArrayDeserializer {
    iter: std::vec::IntoIter<Data<'static>>,
}

impl OwnedArrayDeserializer {
    fn new(slice: Vec<Data<'static>>) -> Self {
        Self {
            iter: slice.into_iter(),
        }
    }
}

impl<'de> serde::de::SeqAccess<'de> for OwnedArrayDeserializer {
    type Error = DataDeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }
}

// MapAccess implementation for Data::Object
struct ObjectDeserializer<'de> {
    iter: std::collections::btree_map::Iter<'de, SmolStr, Data<'de>>,
    value: Option<&'de Data<'de>>,
}

impl<'de> ObjectDeserializer<'de> {
    fn new(map: &'de BTreeMap<SmolStr, Data<'de>>) -> Self {
        Self {
            iter: map.iter(),
            value: None,
        }
    }
}

impl<'de> serde::de::MapAccess<'de> for ObjectDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(BorrowedStrDeserializer(key.as_str()))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(value),
            None => Err(DataDeserializerError::Message(
                "value is missing".to_string(),
            )),
        }
    }
}

// MapAccess implementation for Data::Object
struct OwnedObjectDeserializer {
    iter: std::collections::btree_map::IntoIter<SmolStr, Data<'static>>,
    value: Option<Data<'static>>,
}

impl OwnedObjectDeserializer {
    fn new(map: BTreeMap<SmolStr, Data<'static>>) -> Self {
        Self {
            iter: map.into_iter(),
            value: None,
        }
    }
}

impl<'de> serde::de::MapAccess<'de> for OwnedObjectDeserializer {
    type Error = DataDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(OwnedStrDeserializer(key)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(value),
            None => Err(DataDeserializerError::Message(
                "value is missing".to_string(),
            )),
        }
    }
}

// SeqAccess implementation for RawData::Array
struct RawArrayDeserializer<'de> {
    iter: std::slice::Iter<'de, RawData<'de>>,
}

impl<'de> RawArrayDeserializer<'de> {
    fn new(slice: &'de [RawData<'de>]) -> Self {
        Self { iter: slice.iter() }
    }
}

impl<'de> serde::de::SeqAccess<'de> for RawArrayDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }
}

// SeqAccess implementation for RawData::Array
struct RawOwnedArrayDeserializer<'de> {
    iter: std::vec::IntoIter<RawData<'de>>,
}

impl<'de> RawOwnedArrayDeserializer<'de> {
    fn new(data: Vec<RawData<'de>>) -> Self {
        Self {
            iter: data.into_iter(),
        }
    }
}

impl<'de> serde::de::SeqAccess<'de> for RawOwnedArrayDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value.into_static()).map(Some),
            None => Ok(None),
        }
    }
}

// MapAccess implementation for RawData::Object
struct RawObjectDeserializer<'de> {
    iter: std::collections::btree_map::Iter<'de, SmolStr, RawData<'de>>,
    value: Option<&'de RawData<'de>>,
}

impl<'de> RawObjectDeserializer<'de> {
    fn new(map: &'de BTreeMap<SmolStr, RawData<'de>>) -> Self {
        Self {
            iter: map.iter(),
            value: None,
        }
    }
}

impl<'de> serde::de::MapAccess<'de> for RawObjectDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(BorrowedStrDeserializer(key.as_str()))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(value),
            None => Err(DataDeserializerError::Message(
                "value is missing".to_string(),
            )),
        }
    }
}

// MapAccess implementation for RawData::Object
struct RawOwnedObjectDeserializer<'de> {
    iter: std::collections::btree_map::IntoIter<SmolStr, RawData<'de>>,
    value: Option<RawData<'de>>,
}

impl<'de> RawOwnedObjectDeserializer<'de> {
    fn new(map: BTreeMap<SmolStr, RawData<'de>>) -> Self {
        Self {
            iter: map.into_iter(),
            value: None,
        }
    }
}

impl<'de> serde::de::MapAccess<'de> for RawOwnedObjectDeserializer<'de> {
    type Error = DataDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(OwnedStrDeserializer(key)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(value.into_static()),
            None => Err(DataDeserializerError::Message(
                "value is missing".to_string(),
            )),
        }
    }
}

/// Error type for RawData serialization
#[derive(Debug)]
pub enum RawDataSerializerError {
    /// Error message
    Message(String),
}

impl fmt::Display for RawDataSerializerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Message(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for RawDataSerializerError {}

impl serde::ser::Error for RawDataSerializerError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::Message(msg.to_string())
    }
}

/// Serializer that produces RawData values
pub struct RawDataSerializer;

impl serde::Serializer for RawDataSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    type SerializeSeq = RawDataSeqSerializer;
    type SerializeTuple = RawDataSeqSerializer;
    type SerializeTupleStruct = RawDataSeqSerializer;
    type SerializeTupleVariant = RawDataSeqSerializer;
    type SerializeMap = RawDataMapSerializer;
    type SerializeStruct = RawDataMapSerializer;
    type SerializeStructVariant = RawDataMapSerializer;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Boolean(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::SignedInt(v as i64))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::SignedInt(v as i64))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::SignedInt(v as i64))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::SignedInt(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::UnsignedInt(v as u64))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::UnsignedInt(v as u64))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::UnsignedInt(v as u64))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::UnsignedInt(v))
    }

    fn serialize_f32(self, _v: f32) -> Result<Self::Ok, Self::Error> {
        Err(RawDataSerializerError::Message(
            "floating point numbers not supported in AT Protocol data".to_string(),
        ))
    }

    fn serialize_f64(self, _v: f64) -> Result<Self::Ok, Self::Error> {
        Err(RawDataSerializerError::Message(
            "floating point numbers not supported in AT Protocol data".to_string(),
        ))
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::String(v.to_string().into()))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::String(CowStr::Owned(v.to_smolstr())))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Bytes(Bytes::copy_from_slice(v)))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Null)
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Null)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::String(CowStr::Owned(variant.to_smolstr())))
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        let mut map = BTreeMap::new();
        map.insert(variant.to_smolstr(), value.serialize(RawDataSerializer)?);
        Ok(RawData::Object(map))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(RawDataSeqSerializer {
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(RawDataMapSerializer {
            map: BTreeMap::new(),
            next_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.serialize_map(Some(len))
    }
}

/// Sequence serializer accumulator
pub struct RawDataSeqSerializer {
    items: Vec<RawData<'static>>,
}

impl serde::ser::SerializeSeq for RawDataSeqSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.items.push(value.serialize(RawDataSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Array(self.items))
    }
}

impl serde::ser::SerializeTuple for RawDataSeqSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl serde::ser::SerializeTupleStruct for RawDataSeqSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl serde::ser::SerializeTupleVariant for RawDataSeqSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeSeq::end(self)
    }
}

/// Map serializer accumulator
pub struct RawDataMapSerializer {
    map: BTreeMap<SmolStr, RawData<'static>>,
    next_key: Option<SmolStr>,
}

impl serde::ser::SerializeMap for RawDataMapSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        let key_data = key.serialize(RawDataSerializer)?;
        match key_data {
            RawData::String(s) => {
                self.next_key = Some(s.to_smolstr());
                Ok(())
            }
            _ => Err(RawDataSerializerError::Message(
                "map keys must be strings".to_string(),
            )),
        }
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| RawDataSerializerError::Message("missing key".to_string()))?;
        self.map.insert(key, value.serialize(RawDataSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Object(self.map))
    }
}

impl serde::ser::SerializeStruct for RawDataMapSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.map
            .insert(key.to_smolstr(), value.serialize(RawDataSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(RawData::Object(self.map))
    }
}

impl serde::ser::SerializeStructVariant for RawDataMapSerializer {
    type Ok = RawData<'static>;
    type Error = RawDataSerializerError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        serde::ser::SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeStruct::end(self)
    }
}
