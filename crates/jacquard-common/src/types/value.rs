use crate::types::{DataModelType, blob::Blob, string::*};
use bytes::Bytes;
use ipld_core::ipld::Ipld;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::BTreeMap;

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
    pub fn from_json(json: &'s serde_json::Value) -> Result<Self, AtDataError> {
        Ok(if let Some(value) = json.as_bool() {
            Self::Boolean(value)
        } else if let Some(value) = json.as_i64() {
            Self::Integer(value)
        } else if let Some(value) = json.as_str() {
            Self::String(AtprotoStr::new(value))
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
            Ipld::String(string) => Self::String(AtprotoStr::new(string)),
            Ipld::Bytes(items) => Self::Bytes(Bytes::copy_from_slice(items.as_slice())),
            Ipld::List(iplds) => Self::Array(Array::from_cbor(iplds)?),
            Ipld::Map(btree_map) => Object::from_cbor(btree_map)?,
            Ipld::Link(cid) => Self::CidLink(Cid::ipld(*cid)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array<'s>(pub Vec<Data<'s>>);

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
