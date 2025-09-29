use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;
use std::collections::BTreeMap;

use crate::types::{blob::Blob, string::*};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data<'s> {
    Null,
    Boolean(bool),
    Integer(i64),
    String(AtprotoStr<'s>),
    Bytes(Bytes),     // maybe need custom type for serialization
    CidLink(Cid<'s>), // maybe need custom type for serialization
    Array(Array<'s>),
    Object(Object<'s>),
    Blob(Blob<'s>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array<'s>(pub Vec<Data<'s>>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<'s>(pub BTreeMap<SmolStr, Data<'s>>);
