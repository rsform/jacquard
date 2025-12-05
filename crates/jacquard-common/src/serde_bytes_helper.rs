//! Custom serde helpers for bytes::Bytes using serde_bytes

use bytes::Bytes;
use serde::{Deserializer, Serializer};

/// Serialize Bytes as a CBOR byte string
pub fn serialize<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serde_bytes::serialize(bytes.as_ref(), serializer)
}

/// Deserialize Bytes from a CBOR byte string
pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    Ok(Bytes::from(vec))
}
