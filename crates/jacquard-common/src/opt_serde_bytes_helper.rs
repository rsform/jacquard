//! Custom serde helpers for bytes::Bytes using serde_bytes

use base64::{
    Engine,
    prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD, BASE64_URL_SAFE, BASE64_URL_SAFE_NO_PAD},
};
use bytes::Bytes;
use core::fmt;
use serde::{
    Deserializer, Serializer,
    de::{self, MapAccess, Visitor},
};

/// Serialize Bytes as a CBOR byte string
pub fn serialize<S>(bytes: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(bytes) = bytes {
        if serializer.is_human_readable() {
            // JSON: {"$bytes": "base64 string"}
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("$bytes", &BASE64_STANDARD.encode(bytes))?;
            map.end()
        } else {
            // CBOR: raw bytes
            serde_bytes::serialize(bytes.as_ref(), serializer)
        }
    } else {
        serializer.serialize_none()
    }
}

/// Deserialize Bytes from a CBOR byte string
pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Bytes>, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        Ok(deserializer.deserialize_map(OptBytesVisitor)?)
    } else {
        let vec: Option<Vec<u8>> = serde_bytes::deserialize(deserializer)?;
        Ok(vec.map(Bytes::from))
    }
}

struct OptBytesVisitor;

impl<'de> Visitor<'de> for OptBytesVisitor {
    type Value = Option<Bytes>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a base64-encoded string")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Ok(Some(Bytes::from(vec)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut bytes = None;

        while let Some(key) = map.next_key()? {
            match key {
                "$bytes" => {
                    if bytes.is_some() {
                        return Err(de::Error::duplicate_field("$bytes"));
                    }
                    let bytes_str: String = map.next_value()?;
                    // First one should just work. rest are insurance.
                    bytes = if let Ok(bytes) = BASE64_STANDARD.decode(&bytes_str) {
                        Some(Bytes::from_owner(bytes))
                    } else if let Ok(bytes) = BASE64_STANDARD_NO_PAD.decode(&bytes_str) {
                        Some(Bytes::from_owner(bytes))
                    } else if let Ok(bytes) = BASE64_URL_SAFE.decode(&bytes_str) {
                        Some(Bytes::from_owner(bytes))
                    } else if let Ok(bytes) = BASE64_URL_SAFE_NO_PAD.decode(&bytes_str) {
                        Some(Bytes::from_owner(bytes))
                    } else {
                        return Err(de::Error::custom("invalid base64 string"));
                    }
                }
                _ => {
                    return Err(de::Error::unknown_field(key, &["$bytes"]));
                }
            }
        }

        Ok(bytes)
    }
}
