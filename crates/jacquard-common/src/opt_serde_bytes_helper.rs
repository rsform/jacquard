//! Custom serde helpers for bytes::Bytes using serde_bytes

use bytes::Bytes;
use serde::{Deserializer, Serializer};

/// Serialize Bytes as a CBOR byte string
pub fn serialize<S>(bytes: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(bytes) = bytes {
        serde_bytes::serialize(bytes.as_ref(), serializer)
    } else {
        serializer.serialize_none()
    }
}

/// Deserialize Bytes from a CBOR byte string
pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Bytes>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Option<Vec<u8>> = serde_bytes::deserialize(deserializer)?;
    Ok(vec.map(Bytes::from))
}
