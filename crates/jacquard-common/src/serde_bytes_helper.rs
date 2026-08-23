//! Custom serde helpers for bytes::Bytes using serde_bytes

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use base64::{
    Engine,
    prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD, BASE64_URL_SAFE, BASE64_URL_SAFE_NO_PAD},
};
use bytes::Bytes;
use serde::{
    Deserializer, Serializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};

/// Serialize Bytes as a CBOR byte string
pub fn serialize<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
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
}

/// Deserialize Bytes from a CBOR byte string
pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        deserializer.deserialize_any(BytesVisitor)
    } else {
        let vec: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Ok(Bytes::from(vec))
    }
}

struct BytesVisitor;

impl<'de> Visitor<'de> for BytesVisitor {
    type Value = Bytes;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a base64-encoded $bytes object or structured bytes")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Bytes::copy_from_slice(v))
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Bytes::copy_from_slice(v))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Bytes::from_owner(v))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut value = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(byte) = seq.next_element()? {
            value.push(byte);
        }
        Ok(Bytes::from(value))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut bytes = None;

        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            match key.as_ref() {
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
                other => {
                    return Err(de::Error::unknown_field(other, &["$bytes"]));
                }
            }
        }

        bytes.ok_or_else(|| de::Error::missing_field("$bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct BytePayload {
        #[serde(with = "super")]
        bytes: Bytes,
    }

    #[test]
    fn json_from_value_accepts_owned_map_keys() {
        let value = serde_json::json!({
            "bytes": {"$bytes": BASE64_STANDARD.encode([0, 1, 2, 255])}
        });
        let parsed: BytePayload = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.bytes, Bytes::from_static(&[0, 1, 2, 255]));
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "$type")]
    enum TaggedPayload {
        #[serde(rename = "com.example.bytes")]
        Bytes(BytePayload),
    }

    #[test]
    fn dag_cbor_bytes_survive_internally_tagged_buffering() {
        let value = TaggedPayload::Bytes(BytePayload {
            bytes: Bytes::from_static(&[0, 1, 2, 255]),
        });
        let encoded = serde_ipld_dagcbor::to_vec(&value).unwrap();
        let decoded: TaggedPayload = serde_ipld_dagcbor::from_slice(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
