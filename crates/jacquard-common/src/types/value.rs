use crate::{
    IntoStatic,
    types::{DataModelType, LexiconStringType, UriType, blob::Blob, string::*},
};
use bytes::Bytes;
use ipld_core::ipld::Ipld;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::BTreeMap;

/// Conversion utilities for Data types
pub mod convert;
/// String parsing for AT Protocol types
pub mod parsing;
/// Serde implementations for Data types
pub mod serde_impl;

pub use serde_impl::{DataDeserializerError, RawDataSerializerError};

#[cfg(test)]
mod tests;

/// AT Protocol data model value
///
/// Represents any valid value in the AT Protocol data model, which supports JSON and CBOR
/// serialization with specific constraints (no floats, CID links, blobs with metadata).
///
/// This is the generic "unknown data" type used for lexicon values, extra fields captured
/// by `#[lexicon]`, and IPLD data structures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data<'s> {
    /// Null value
    Null,
    /// Boolean value
    Boolean(bool),
    /// Integer value (no floats in AT Protocol)
    Integer(i64),
    /// String value (parsed into specific AT Protocol types when possible)
    String(AtprotoStr<'s>),
    /// Raw bytes
    Bytes(Bytes),
    /// CID link reference
    CidLink(Cid<'s>),
    /// Array of values
    Array(Array<'s>),
    /// Object/map of values
    Object(Object<'s>),
    /// Blob reference with metadata
    Blob(Blob<'s>),
}

/// Errors that can occur when working with AT Protocol data
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
pub enum AtDataError {
    /// Floating point numbers are not allowed in AT Protocol
    #[error("floating point numbers not allowed in AT protocol data")]
    FloatNotAllowed,
}

impl<'s> Data<'s> {
    /// Get the data model type of this value
    pub fn data_type(&self) -> DataModelType {
        match self {
            Data::Null => DataModelType::Null,
            Data::Boolean(_) => DataModelType::Boolean,
            Data::Integer(_) => DataModelType::Integer,
            Data::String(s) => match s {
                AtprotoStr::Datetime(_) => DataModelType::String(LexiconStringType::Datetime),
                AtprotoStr::Language(_) => DataModelType::String(LexiconStringType::Language),
                AtprotoStr::Tid(_) => DataModelType::String(LexiconStringType::Tid),
                AtprotoStr::Nsid(_) => DataModelType::String(LexiconStringType::Nsid),
                AtprotoStr::Did(_) => DataModelType::String(LexiconStringType::Did),
                AtprotoStr::Handle(_) => DataModelType::String(LexiconStringType::Handle),
                AtprotoStr::AtIdentifier(_) => {
                    DataModelType::String(LexiconStringType::AtIdentifier)
                }
                AtprotoStr::AtUri(_) => DataModelType::String(LexiconStringType::AtUri),
                AtprotoStr::Uri(uri) => match uri {
                    Uri::Did(_) => DataModelType::String(LexiconStringType::Uri(UriType::Did)),
                    Uri::At(_) => DataModelType::String(LexiconStringType::Uri(UriType::At)),
                    Uri::Https(_) => DataModelType::String(LexiconStringType::Uri(UriType::Https)),
                    Uri::Wss(_) => DataModelType::String(LexiconStringType::Uri(UriType::Wss)),
                    Uri::Cid(_) => DataModelType::String(LexiconStringType::Uri(UriType::Cid)),
                    Uri::Any(_) => DataModelType::String(LexiconStringType::Uri(UriType::Any)),
                },
                AtprotoStr::Cid(_) => DataModelType::String(LexiconStringType::Cid),
                AtprotoStr::RecordKey(_) => DataModelType::String(LexiconStringType::RecordKey),
                AtprotoStr::String(_) => DataModelType::String(LexiconStringType::String),
            },
            Data::Bytes(_) => DataModelType::Bytes,
            Data::CidLink(_) => DataModelType::CidLink,
            Data::Array(_) => DataModelType::Array,
            Data::Object(_) => DataModelType::Object,
            Data::Blob(_) => DataModelType::Blob,
        }
    }
    /// Parse a Data value from a JSON value
    pub fn from_json(json: &'s serde_json::Value) -> Result<Self, AtDataError> {
        Ok(if let Some(value) = json.as_bool() {
            Self::Boolean(value)
        } else if let Some(value) = json.as_i64() {
            Self::Integer(value)
        } else if let Some(value) = json.as_str() {
            Self::String(parsing::parse_string(value))
        } else if let Some(value) = json.as_array() {
            Self::Array(Array::from_json(value)?)
        } else if let Some(value) = json.as_object() {
            Object::from_json(value)?
        } else if json.is_f64() {
            return Err(AtDataError::FloatNotAllowed);
        } else {
            Self::Null
        })
    }

    /// Parse a Data value from a JSON value (owned)
    pub fn from_json_owned(json: serde_json::Value) -> Result<Data<'static>, AtDataError> {
        Data::from_json(&json).map(|data| data.into_static())
    }

    /// Parse a Data value from an IPLD value (CBOR)
    pub fn from_cbor(cbor: &'s Ipld) -> Result<Self, AtDataError> {
        Ok(match cbor {
            Ipld::Null => Data::Null,
            Ipld::Bool(bool) => Data::Boolean(*bool),
            Ipld::Integer(int) => Data::Integer(*int as i64),
            Ipld::Float(_) => {
                return Err(AtDataError::FloatNotAllowed);
            }
            Ipld::String(string) => Self::String(parsing::parse_string(string)),
            Ipld::Bytes(items) => Self::Bytes(Bytes::copy_from_slice(items.as_slice())),
            Ipld::List(iplds) => Self::Array(Array::from_cbor(iplds)?),
            Ipld::Map(btree_map) => Object::from_cbor(btree_map)?,
            Ipld::Link(cid) => Self::CidLink(Cid::ipld(*cid)),
        })
    }
}

impl IntoStatic for Data<'_> {
    type Output = Data<'static>;
    fn into_static(self) -> Data<'static> {
        match self {
            Data::Null => Data::Null,
            Data::Boolean(bool) => Data::Boolean(bool),
            Data::Integer(int) => Data::Integer(int),
            Data::String(string) => Data::String(string.into_static()),
            Data::Bytes(bytes) => Data::Bytes(bytes),
            Data::Array(array) => Data::Array(array.into_static()),
            Data::Object(object) => Data::Object(object.into_static()),
            Data::CidLink(cid) => Data::CidLink(cid.into_static()),
            Data::Blob(blob) => Data::Blob(blob.into_static()),
        }
    }
}

/// Array of AT Protocol data values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array<'s>(pub Vec<Data<'s>>);

impl IntoStatic for Array<'_> {
    type Output = Array<'static>;
    fn into_static(self) -> Array<'static> {
        Array(self.0.into_static())
    }
}

impl<'s> Array<'s> {
    /// Parse an array from JSON values
    pub fn from_json(json: &'s Vec<serde_json::Value>) -> Result<Self, AtDataError> {
        let mut array = Vec::with_capacity(json.len());
        for item in json {
            array.push(Data::from_json(item)?);
        }
        Ok(Self(array))
    }
    /// Parse an array from IPLD values (CBOR)
    pub fn from_cbor(cbor: &'s Vec<Ipld>) -> Result<Self, AtDataError> {
        let mut array = Vec::with_capacity(cbor.len());
        for item in cbor {
            array.push(Data::from_cbor(item)?);
        }
        Ok(Self(array))
    }
}

/// Object/map of AT Protocol data values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<'s>(pub BTreeMap<SmolStr, Data<'s>>);

impl IntoStatic for Object<'_> {
    type Output = Object<'static>;
    fn into_static(self) -> Object<'static> {
        Object(self.0.into_static())
    }
}

impl<'s> Object<'s> {
    /// Parse an object from a JSON map with type inference
    ///
    /// Uses key names to infer the appropriate AT Protocol types for values.
    pub fn from_json(
        json: &'s serde_json::Map<String, serde_json::Value>,
    ) -> Result<Data<'s>, AtDataError> {
        if let Some(type_field) = json.get("$type").and_then(|v| v.as_str()) {
            if parsing::infer_from_type(type_field) == DataModelType::Blob {
                if let Some(blob) = parsing::json_to_blob(json) {
                    return Ok(Data::Blob(blob));
                }
            }
        }
        let mut map = BTreeMap::new();

        for (key, value) in json {
            if key == "$type" {
                map.insert(key.to_smolstr(), Data::from_json(value)?);
            }
            match parsing::string_key_type_guess(key) {
                DataModelType::Null if value.is_null() => {
                    map.insert(key.to_smolstr(), Data::Null);
                }
                DataModelType::Boolean if value.is_boolean() => {
                    map.insert(key.to_smolstr(), Data::Boolean(value.as_bool().unwrap()));
                }
                DataModelType::Integer if value.is_i64() => {
                    map.insert(key.to_smolstr(), Data::Integer(value.as_i64().unwrap()));
                }
                DataModelType::Bytes if value.is_string() => {
                    map.insert(
                        key.to_smolstr(),
                        parsing::decode_bytes(value.as_str().unwrap()),
                    );
                }
                DataModelType::CidLink => {
                    if let Some(value) = value.as_object() {
                        if let Some(value) = value.get("$link").and_then(|v| v.as_str()) {
                            map.insert(key.to_smolstr(), Data::CidLink(Cid::Str(value.into())));
                        } else {
                            map.insert(key.to_smolstr(), Object::from_json(value)?);
                        }
                    } else {
                        map.insert(key.to_smolstr(), Data::from_json(value)?);
                    }
                }
                DataModelType::Blob if value.is_object() => {
                    map.insert(
                        key.to_smolstr(),
                        Object::from_json(value.as_object().unwrap())?,
                    );
                }
                DataModelType::Array if value.is_array() => {
                    map.insert(
                        key.to_smolstr(),
                        Data::Array(Array::from_json(value.as_array().unwrap())?),
                    );
                }
                DataModelType::Object if value.is_object() => {
                    map.insert(
                        key.to_smolstr(),
                        Object::from_json(value.as_object().unwrap())?,
                    );
                }
                DataModelType::String(string_type) if value.is_string() => {
                    parsing::insert_string(&mut map, key, value.as_str().unwrap(), string_type)?;
                }
                _ => {
                    map.insert(key.to_smolstr(), Data::from_json(value)?);
                }
            }
        }

        Ok(Data::Object(Object(map)))
    }

    /// Parse an object from IPLD (CBOR) with type inference
    ///
    /// Uses key names to infer the appropriate AT Protocol types for values.
    pub fn from_cbor(cbor: &'s BTreeMap<String, Ipld>) -> Result<Data<'s>, AtDataError> {
        if let Some(Ipld::String(type_field)) = cbor.get("$type") {
            if parsing::infer_from_type(type_field) == DataModelType::Blob {
                if let Some(blob) = parsing::cbor_to_blob(cbor) {
                    return Ok(Data::Blob(blob));
                }
            }
        }
        let mut map = BTreeMap::new();

        for (key, value) in cbor {
            if key == "$type" {
                map.insert(key.to_smolstr(), Data::from_cbor(value)?);
            }
            match (parsing::string_key_type_guess(key), value) {
                (DataModelType::Null, Ipld::Null) => {
                    map.insert(key.to_smolstr(), Data::Null);
                }
                (DataModelType::Boolean, Ipld::Bool(value)) => {
                    map.insert(key.to_smolstr(), Data::Boolean(*value));
                }
                (DataModelType::Integer, Ipld::Integer(int)) => {
                    map.insert(key.to_smolstr(), Data::Integer(*int as i64));
                }
                (DataModelType::Bytes, Ipld::Bytes(value)) => {
                    map.insert(key.to_smolstr(), Data::Bytes(Bytes::copy_from_slice(value)));
                }
                (DataModelType::Blob, Ipld::Map(value)) => {
                    map.insert(key.to_smolstr(), Object::from_cbor(value)?);
                }
                (DataModelType::Array, Ipld::List(value)) => {
                    map.insert(key.to_smolstr(), Data::Array(Array::from_cbor(value)?));
                }
                (DataModelType::Object, Ipld::Map(value)) => {
                    map.insert(key.to_smolstr(), Object::from_cbor(value)?);
                }
                (DataModelType::String(string_type), Ipld::String(value)) => {
                    parsing::insert_string(&mut map, key, value, string_type)?;
                }
                _ => {
                    map.insert(key.to_smolstr(), Data::from_cbor(value)?);
                }
            }
        }

        Ok(Data::Object(Object(map)))
    }
}

/// Level 1 deserialization of raw atproto data
///
/// Maximally permissive with zero inference for cases where you just want to pass through the data
/// and don't necessarily care if it's totally valid, or you want to validate later.
/// E.g. lower-level services, PDS implementations, firehose indexers, relay implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawData<'s> {
    /// Null value
    Null,
    /// Boolean value
    Boolean(bool),
    /// Signed integer
    SignedInt(i64),
    /// Unsigned integer
    UnsignedInt(u64),
    /// String value (no type inference)
    String(CowStr<'s>),
    /// Raw bytes
    Bytes(Bytes),
    /// CID link reference
    CidLink(Cid<'s>),
    /// Array of raw values
    Array(Vec<RawData<'s>>),
    /// Object/map of raw values
    Object(BTreeMap<SmolStr, RawData<'s>>),
    /// Valid blob reference
    Blob(Blob<'s>),
    /// Invalid blob structure (captured for debugging)
    InvalidBlob(Box<RawData<'s>>),
    /// Invalid number format, generally a floating point number (captured as bytes)
    InvalidNumber(Bytes),
    /// Invalid/unknown data (captured as bytes)
    InvalidData(Bytes),
}

impl IntoStatic for RawData<'_> {
    type Output = RawData<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            RawData::Null => RawData::Null,
            RawData::Boolean(b) => RawData::Boolean(b),
            RawData::SignedInt(i) => RawData::SignedInt(i),
            RawData::UnsignedInt(u) => RawData::UnsignedInt(u),
            RawData::String(s) => RawData::String(s.into_static()),
            RawData::Bytes(b) => RawData::Bytes(b.into_static()),
            RawData::CidLink(c) => RawData::CidLink(c.into_static()),
            RawData::Array(a) => RawData::Array(a.into_static()),
            RawData::Object(o) => RawData::Object(o.into_static()),
            RawData::Blob(b) => RawData::Blob(b.into_static()),
            RawData::InvalidBlob(b) => RawData::InvalidBlob(b.into_static()),
            RawData::InvalidNumber(b) => RawData::InvalidNumber(b.into_static()),
            RawData::InvalidData(b) => RawData::InvalidData(b.into_static()),
        }
    }
}

/// Deserialize a typed value from a `Data` value
///
/// Allows extracting strongly-typed structures from untyped `Data` values,
/// similar to `serde_json::from_value()`.
///
/// # Example
/// ```
/// # use jacquard_common::types::value::{Data, from_data};
/// # use serde::Deserialize;
/// #
/// #[derive(Deserialize)]
/// struct Post<'a> {
///     #[serde(borrow)]
///     text: &'a str,
///     #[serde(borrow)]
///     author: &'a str,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # let json = serde_json::json!({"text": "hello", "author": "alice"});
/// # let data = Data::from_json(&json)?;
/// let post: Post = from_data(&data)?;
/// # Ok(())
/// # }
/// ```
pub fn from_data<'de, T>(data: &'de Data<'de>) -> Result<T, DataDeserializerError>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(data)
}

/// Deserialize a typed value from a `Data` value
///
/// Takes ownership rather than borrows. Will allocate.
pub fn from_data_owned<'de, T>(data: Data<'_>) -> Result<T, DataDeserializerError>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(data.into_static())
}

/// Deserialize a typed value from a `serde_json::Value`
///
/// Returns an owned version, will allocate
pub fn from_json_value<'de, T>(
    json: serde_json::Value,
) -> Result<<T as IntoStatic>::Output, serde_json::Error>
where
    T: serde::Deserialize<'de> + IntoStatic,
{
    T::deserialize(json).map(IntoStatic::into_static)
}

/// Deserialize a typed value from a `RawData` value
///
/// Allows extracting strongly-typed structures from untyped `RawData` values.
///
/// # Example
/// ```
/// # use jacquard_common::types::value::{RawData, from_raw_data, to_raw_data};
/// # use serde::{Serialize, Deserialize};
/// #
/// #[derive(Serialize, Deserialize)]
/// struct Post {
///     text: String,
///     author: String,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # let orig = Post { text: "hello".to_string(), author: "alice".to_string() };
/// # let data = to_raw_data(&orig)?;
/// let post: Post = from_raw_data(&data)?;
/// # Ok(())
/// # }
/// ```
pub fn from_raw_data<'de, T>(data: &'de RawData<'de>) -> Result<T, DataDeserializerError>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(data)
}

/// Deserialize a typed value from a `RawData` value
///
/// Takes ownership rather than borrows. Will allocate.
pub fn from_raw_data_owned<'de, T>(data: RawData<'_>) -> Result<T, DataDeserializerError>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(data.into_static())
}

/// Serialize a typed value into a `RawData` value
///
/// Allows converting strongly-typed structures into untyped `RawData` values.
///
/// # Example
/// ```
/// # use jacquard_common::types::value::{RawData, to_raw_data};
/// # use serde::Serialize;
/// #
/// #[derive(Serialize)]
/// struct Post {
///     text: String,
///     likes: i64,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let post = Post { text: "hello".to_string(), likes: 42 };
/// let data: RawData = to_raw_data(&post)?;
/// # Ok(())
/// # }
/// ```
pub fn to_raw_data<T>(value: &T) -> Result<RawData<'static>, serde_impl::RawDataSerializerError>
where
    T: serde::Serialize,
{
    value.serialize(serde_impl::RawDataSerializer)
}

/// Serialize a typed value into a validated `Data` value with type inference
///
/// Combines `to_raw_data()` and validation/type inference in one step.
///
/// # Example
/// ```
/// # use jacquard_common::types::value::{Data, to_data};
/// # use serde::Serialize;
/// #
/// #[derive(Serialize)]
/// struct Post {
///     text: String,
///     did: String,  // Will be inferred as Did if valid
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let post = Post {
///     text: "hello".to_string(),
///     did: "did:plc:abc123".to_string()
/// };
/// let data: Data = to_data(&post)?;
/// # Ok(())
/// # }
/// ```
pub fn to_data<T>(value: &T) -> Result<Data<'static>, convert::ConversionError>
where
    T: serde::Serialize,
{
    let raw = to_raw_data(value).map_err(|e| convert::ConversionError::InvalidRawData {
        message: e.to_string(),
    })?;
    raw.try_into()
}
