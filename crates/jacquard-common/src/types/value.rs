use crate::{
    IntoStatic,
    types::{DataModelType, LexiconStringType, UriType, blob::Blob, string::*},
};
use bytes::Bytes;
use ipld_core::ipld::Ipld;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::BTreeMap;

pub mod convert;
pub mod parsing;
pub mod serde_impl;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data<'s> {
    Null,
    Boolean(bool),
    Integer(i64),
    String(AtprotoStr<'s>),
    Bytes(Bytes),
    CidLink(Cid<'s>),
    Array(Array<'s>),
    Object(Object<'s>),
    Blob(Blob<'s>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
pub enum AtDataError {
    #[error("floating point numbers not allowed in AT protocol data")]
    FloatNotAllowed,
}

impl<'s> Data<'s> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array<'s>(pub Vec<Data<'s>>);

impl IntoStatic for Array<'_> {
    type Output = Array<'static>;
    fn into_static(self) -> Array<'static> {
        Array(self.0.into_static())
    }
}

impl<'s> Array<'s> {
    pub fn from_json(json: &'s Vec<serde_json::Value>) -> Result<Self, AtDataError> {
        let mut array = Vec::with_capacity(json.len());
        for item in json {
            array.push(Data::from_json(item)?);
        }
        Ok(Self(array))
    }
    pub fn from_cbor(cbor: &'s Vec<Ipld>) -> Result<Self, AtDataError> {
        let mut array = Vec::with_capacity(cbor.len());
        for item in cbor {
            array.push(Data::from_cbor(item)?);
        }
        Ok(Self(array))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<'s>(pub BTreeMap<SmolStr, Data<'s>>);

impl IntoStatic for Object<'_> {
    type Output = Object<'static>;
    fn into_static(self) -> Object<'static> {
        Object(self.0.into_static())
    }
}

impl<'s> Object<'s> {
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
    Null,
    Boolean(bool),
    SignedInt(i64),
    UnsignedInt(u64),
    String(CowStr<'s>),
    Bytes(Bytes),
    CidLink(Cid<'s>),
    Array(Vec<RawData<'s>>),
    Object(BTreeMap<SmolStr, RawData<'s>>),
    Blob(Blob<'s>),
    InvalidBlob(Box<RawData<'s>>),
    InvalidNumber(Bytes),
    InvalidData(Bytes),
}
