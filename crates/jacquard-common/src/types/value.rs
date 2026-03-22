use crate::{
    Bos, DefaultStr, IntoStatic,
    types::{DataModelType, LexiconStringType, UriType, blob::Blob, string::*},
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;
use bytes::Bytes;
use core::convert::Infallible;
use ipld_core::ipld::Ipld;
use serde::Serialize;
use smol_str::{SmolStr, ToSmolStr};

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
pub enum Data<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// Null value
    Null,
    /// Boolean value
    Boolean(bool),
    /// Integer value (no floats in AT Protocol)
    Integer(i64),
    /// String value (parsed into specific AT Protocol types when possible)
    String(AtprotoStr<S>),
    /// Raw bytes
    Bytes(Bytes),
    /// CID link reference
    CidLink(Cid<S>),
    /// Array of values
    Array(Array<S>),
    /// Object/map of values
    Object(Object<S>),
    /// Blob reference with metadata
    Blob(Blob<S>),
    /// Invalid number (floating point)
    InvalidNumber(S),
}

/// Errors that can occur when working with AT Protocol data
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum AtDataError {
    /// Floating point numbers are not allowed in AT Protocol
    #[error("floating point numbers not allowed in AT protocol data")]
    FloatNotAllowed,
    /// Invalid data type for AT Protocol data
    #[error("invalid data type for AT protocol data")]
    InvalidType,
    /// Deserialization error
    #[error("deserialization error")]
    Deserialization,
}

impl<S> Data<S>
where
    S: Bos<str> + AsRef<str>,
{
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
                    UriValue::Did(_) => DataModelType::String(LexiconStringType::Uri(UriType::Did)),
                    UriValue::At(_) => DataModelType::String(LexiconStringType::Uri(UriType::At)),
                    UriValue::Https(_) => {
                        DataModelType::String(LexiconStringType::Uri(UriType::Https))
                    }
                    UriValue::Wss(_) => DataModelType::String(LexiconStringType::Uri(UriType::Wss)),
                    UriValue::Cid(_) => DataModelType::String(LexiconStringType::Uri(UriType::Cid)),
                    UriValue::Any(_) => DataModelType::String(LexiconStringType::Uri(UriType::Any)),
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
            Data::InvalidNumber(_) => DataModelType::Bytes,
        }
    }

    /// Get as object if this is an Object variant
    pub fn as_object(&self) -> Option<&Object<S>> {
        if let Data::Object(obj) = self {
            Some(obj)
        } else {
            None
        }
    }

    /// Get as array if this is an Array variant
    pub fn as_array(&self) -> Option<&Array<S>> {
        if let Data::Array(arr) = self {
            Some(arr)
        } else {
            None
        }
    }

    /// Get as string if this is a String variant
    pub fn as_str(&self) -> Option<&str> {
        if let Data::String(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }

    /// Get as object if this is an Object variant
    pub fn as_object_mut<'a>(&'a mut self) -> Option<&'a mut Object<S>> {
        if let Data::Object(obj) = self {
            Some(obj)
        } else {
            None
        }
    }

    /// Get as array if this is an Array variant
    pub fn as_array_mut<'a>(&'a mut self) -> Option<&'a mut Array<S>> {
        if let Data::Array(arr) = self {
            Some(arr)
        } else {
            None
        }
    }

    /// Get as string if this is a String variant
    pub fn as_str_mut<'s>(&'s mut self) -> Option<&'s mut AtprotoStr<S>> {
        if let Data::String(s) = self {
            Some(s)
        } else {
            None
        }
    }

    /// Get as integer if this is an Integer variant
    pub fn as_integer_mut(&mut self) -> Option<&mut i64> {
        if let Data::Integer(i) = self {
            Some(i)
        } else {
            None
        }
    }

    /// Get a mutable reference to the boolean if this is a Boolean variant
    pub fn as_boolean_mut(&mut self) -> Option<&mut bool> {
        if let Data::Boolean(b) = self {
            Some(b)
        } else {
            None
        }
    }

    /// Get as integer if this is an Integer variant
    pub fn as_integer(&self) -> Option<i64> {
        if let Data::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    /// Get as boolean if this is a Boolean variant
    pub fn as_boolean(&self) -> Option<bool> {
        if let Data::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    /// Check if this is a null value
    pub fn is_null(&self) -> bool {
        matches!(self, Data::Null)
    }

    /// Get the "$type" discriminator field if this is an object with a string "$type" field
    ///
    /// This is a shortcut for union type discrimination in AT Protocol.
    /// Returns `None` if this is not an object or if the "$type" field is missing/not a string.
    pub fn type_discriminator(&self) -> Option<&str> {
        self.as_object()?.type_discriminator()
    }

    /// Serialize to canonical DAG-CBOR bytes for CID computation
    ///
    /// This produces the deterministic CBOR encoding used for content-addressing.
    pub fn to_dag_cbor(
        &self,
    ) -> Result<Vec<u8>, serde_ipld_dagcbor::EncodeError<alloc::collections::TryReserveError>>
    where
        S: Serialize,
    {
        serde_ipld_dagcbor::to_vec(self)
    }

    /// Get a value at a path within nested Data structures
    ///
    /// Path syntax:
    /// - `.field` or `field` - access object field
    /// - `[0]` - access array index
    /// - Combined: `embed.images[0].alt`
    ///
    /// # Example
    /// ```ignore
    /// let data: Data = ...;
    /// if let Some(alt_text) = data.get_at_path("embed.images[0].alt") {
    ///     println!("Alt text: {}", alt_text.as_str().unwrap());
    /// }
    /// ```
    pub fn get_at_path<'s>(&'s self, path: &str) -> Option<&'s Data<S>> {
        parse_and_traverse_path(self, path)
    }

    /// Get a mutable reference to a field at the given path
    ///
    /// Uses the same path syntax as [`get_at_path`](Self::get_at_path).
    pub fn get_at_path_mut(&mut self, path: &str) -> Option<&mut Data<S>> {
        parse_and_traverse_path_mut(self, path)
    }

    /// Set the value at the given path, returning true if successful
    ///
    /// Uses the same path syntax as [`get_at_path`](Self::get_at_path).
    pub fn set_at_path(&mut self, path: &str, new_data: Data<S>) -> bool {
        if let Some(data) = parse_and_traverse_path_mut(self, path) {
            *data = new_data;
            true
        } else {
            false
        }
    }

    /// Query data with pattern matching
    ///
    /// Pattern syntax:
    /// - `field.nested` - exact path navigation
    /// - `[..]` - wildcard over collection (array elements or object values)
    /// - `field..nested` - scoped recursion (find nested within field, expect one)
    /// - `...field` - global recursion (find all occurrences anywhere)
    ///
    /// # Examples
    /// ```ignore
    /// // Exact path with wildcard
    /// let alts = data.query("embed.[..].alt");
    ///
    /// // Scoped recursion
    /// let handle = data.query("post..handle"); // finds post.author.handle
    ///
    /// // Global recursion
    /// let all_cids = data.query("...cid"); // all CIDs anywhere
    /// ```
    pub fn query<'s>(&'s self, pattern: &str) -> QueryResult<'s, S> {
        query_data(self, pattern)
    }
}

impl<S: Bos<str> + AsRef<str>> Data<S> {
    /// Convert to a `Data` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> Data<B> {
        match self {
            Data::Null => Data::Null,
            Data::Boolean(b) => Data::Boolean(b),
            Data::Integer(i) => Data::Integer(i),
            Data::String(s) => Data::String(s.convert()),
            Data::Bytes(b) => Data::Bytes(b),
            Data::CidLink(cid) => Data::CidLink(cid.convert()),
            Data::Array(arr) => Data::Array(arr.convert()),
            Data::Object(obj) => Data::Object(obj.convert()),
            Data::Blob(blob) => Data::Blob(blob.convert()),
            Data::InvalidNumber(float) => Data::InvalidNumber(float.into()),
        }
    }
}

impl<S> IntoStatic for Data<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    <S as IntoStatic>::Output: AsRef<str> + Bos<str>,
{
    type Output = Data<<S as IntoStatic>::Output>;
    fn into_static(self) -> Data<<S as IntoStatic>::Output> {
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
            Data::InvalidNumber(float) => Data::InvalidNumber(float.into_static()),
        }
    }
}

/// Array of AT Protocol data values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Array<S = DefaultStr>(pub Vec<Data<S>>)
where
    S: Bos<str> + AsRef<str>;

impl<S: Bos<str> + AsRef<str>> Array<S> {
    /// Convert to an `Array` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> Array<B> {
        Array(self.0.into_iter().map(|d| d.convert()).collect())
    }
}

impl<S> IntoStatic for Array<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    <S as IntoStatic>::Output: AsRef<str> + Bos<str>,
{
    type Output = Array<<S as IntoStatic>::Output>;
    fn into_static(self) -> Array<<S as IntoStatic>::Output> {
        Array(self.0.into_static())
    }
}

impl<S> Array<S>
where
    S: Bos<str> + AsRef<str>,
{
    /// Get the number of elements in the array
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the array is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get an element by index
    pub fn get(&self, index: usize) -> Option<&Data<S>> {
        self.0.get(index)
    }

    /// Get a mutable reference to an element by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Data<S>> {
        self.0.get_mut(index)
    }

    /// Get an iterator over the array elements
    pub fn iter(&self) -> core::slice::Iter<'_, Data<S>> {
        self.0.iter()
    }
}

impl<S> core::ops::Index<usize> for Array<S>
where
    S: AsRef<str> + Bos<str>,
{
    type Output = Data<S>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

/// Object/map of AT Protocol data values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<S = DefaultStr>(pub BTreeMap<SmolStr, Data<S>>)
where
    S: Bos<str> + AsRef<str>;

impl<S: Bos<str> + AsRef<str>> Object<S> {
    /// Convert to an `Object` with a different backing type.
    pub fn convert<B: Bos<str> + AsRef<str> + From<S>>(self) -> Object<B> {
        Object(self.0.into_iter().map(|(k, v)| (k, v.convert())).collect())
    }
}

impl<S> IntoStatic for Object<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    <S as IntoStatic>::Output: AsRef<str> + Bos<str>,
{
    type Output = Object<<S as IntoStatic>::Output>;
    fn into_static(self) -> Object<<S as IntoStatic>::Output> {
        Object(self.0.into_static())
    }
}

impl<S> Object<S>
where
    S: AsRef<str> + Bos<str>,
{
    /// Get a value by key
    pub fn get(&self, key: &str) -> Option<&Data<S>> {
        self.0.get(key)
    }

    /// Get a mutable reference to a value by key
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Data<S>> {
        self.0.get_mut(key)
    }

    /// Check if a key exists in the object
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Get the number of key-value pairs in the object
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the object is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get an iterator over the key-value pairs
    pub fn iter(&self) -> alloc::collections::btree_map::Iter<'_, SmolStr, Data<S>> {
        self.0.iter()
    }

    /// Get an iterator over the keys
    pub fn keys(&self) -> alloc::collections::btree_map::Keys<'_, SmolStr, Data<S>> {
        self.0.keys()
    }

    /// Get the "$type" discriminator field if present and it's a string
    ///
    /// This is a shortcut for union type discrimination in AT Protocol.
    pub fn type_discriminator(&self) -> Option<&str> {
        self.get("$type")?.as_str()
    }

    /// Get an iterator over the values
    pub fn values(&self) -> alloc::collections::btree_map::Values<'_, SmolStr, Data<S>> {
        self.0.values()
    }
}

impl<S> core::ops::Index<&str> for Object<S>
where
    S: AsRef<str> + Bos<str>,
{
    type Output = Data<S>;

    fn index(&self, key: &str) -> &Self::Output {
        &self.0[key]
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
    CidLink(Cid<CowStr<'s>>),
    /// Array of raw values
    Array(Vec<RawData<'s>>),
    /// Object/map of raw values
    Object(BTreeMap<SmolStr, RawData<'s>>),
    /// Valid blob reference
    Blob(Blob<CowStr<'s>>),
    /// Invalid blob structure (captured for debugging)
    InvalidBlob(Box<RawData<'s>>),
    /// Invalid number format, generally a floating point number (captured as bytes)
    InvalidNumber(Bytes),
    /// Invalid/unknown data (captured as bytes)
    InvalidData(Bytes),
}

impl<'d> RawData<'d> {
    /// Get as object if this is an Object variant
    pub fn as_object(&self) -> Option<&BTreeMap<SmolStr, RawData<'d>>> {
        if let RawData::Object(obj) = self {
            Some(obj)
        } else {
            None
        }
    }

    /// Get as array if this is an Array variant
    pub fn as_array(&self) -> Option<&Vec<RawData<'d>>> {
        if let RawData::Array(arr) = self {
            Some(arr)
        } else {
            None
        }
    }

    /// Get as string if this is a String variant
    pub fn as_str(&self) -> Option<&str> {
        if let RawData::String(s) = self {
            Some(s.as_ref())
        } else {
            None
        }
    }

    /// Get as boolean if this is a Boolean variant
    pub fn as_boolean(&self) -> Option<bool> {
        if let RawData::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    /// get as object if this is an Object variant
    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<SmolStr, RawData<'d>>> {
        if let RawData::Object(obj) = self {
            Some(obj)
        } else {
            None
        }
    }

    /// Get as array if this is an Array variant
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<RawData<'d>>> {
        if let RawData::Array(arr) = self {
            Some(arr)
        } else {
            None
        }
    }

    /// Get as string if this is a String variant
    pub fn as_str_mut(&mut self) -> Option<&mut CowStr<'d>> {
        if let RawData::String(s) = self {
            Some(s)
        } else {
            None
        }
    }

    /// Get as boolean if this is a Boolean variant
    pub fn as_boolean_mut(&mut self) -> Option<&mut bool> {
        if let RawData::Boolean(b) = self {
            Some(b)
        } else {
            None
        }
    }

    /// Check if this is a null value
    pub fn is_null(&self) -> bool {
        matches!(self, RawData::Null)
    }

    /// Get the "$type" discriminator field if this is an object with a string "$type" field
    ///
    /// This is a shortcut for union type discrimination in AT Protocol.
    /// Returns `None` if this is not an object or if the "$type" field is missing/not a string.
    pub fn type_discriminator(&self) -> Option<&str> {
        let obj = self.as_object()?;
        let type_val = obj.get("$type")?;
        type_val.as_str()
    }

    /// Serialize to canonical DAG-CBOR bytes for CID computation
    ///
    /// This produces the deterministic CBOR encoding used for content-addressing.
    pub fn to_dag_cbor(
        &self,
    ) -> Result<Vec<u8>, serde_ipld_dagcbor::EncodeError<alloc::collections::TryReserveError>> {
        serde_ipld_dagcbor::to_vec(self)
    }

    /// Get a value at a path within nested RawData structures
    ///
    /// Path syntax:
    /// - `.field` or `field` - access object field
    /// - `[0]` - access array index
    /// - Combined: `embed.images[0].alt`
    ///
    /// # Example
    /// ```ignore
    /// let data: RawData = ...;
    /// if let Some(alt_text) = data.get_at_path("embed.images[0].alt") {
    ///     println!("Alt text: {}", alt_text.as_str().unwrap());
    /// }
    /// ```
    pub fn get_at_path(&'d self, path: &str) -> Option<&'d RawData<'d>> {
        parse_and_traverse_raw_path(self, path)
    }

    /// Get a mutable reference to a field at the given path
    ///
    /// Uses the same path syntax as [`get_at_path`](Self::get_at_path).
    pub fn get_at_path_mut<'a>(&'a mut self, path: &str) -> Option<&'a mut RawData<'d>> {
        parse_and_traverse_raw_path_mut(self, path)
    }

    /// Convert a CBOR-encoded byte slice into a `RawData` value.
    /// Parse a Data value from an IPLD value (CBOR)
    pub fn from_cbor(cbor: &'d Ipld) -> Result<Self, AtDataError> {
        Ok(match cbor {
            Ipld::Null => RawData::Null,
            Ipld::Bool(bool) => RawData::Boolean(*bool),
            Ipld::Integer(int) => {
                if *int > i64::MAX as i128 {
                    RawData::UnsignedInt(*int as u64)
                } else {
                    RawData::SignedInt(*int as i64)
                }
            }
            Ipld::Float(_) => {
                return Err(AtDataError::FloatNotAllowed);
            }
            Ipld::String(string) => Self::String(CowStr::Borrowed(&string)),
            Ipld::Bytes(items) => Self::Bytes(Bytes::copy_from_slice(items.as_slice())),
            Ipld::List(iplds) => Self::Array(
                iplds
                    .into_iter()
                    .filter_map(|item| RawData::from_cbor(item).ok())
                    .collect(),
            ),
            Ipld::Map(btree_map) => Self::Object(
                btree_map
                    .into_iter()
                    .filter_map(|(key, value)| {
                        if let Ok(value) = RawData::from_cbor(value) {
                            Some((key.to_smolstr(), value))
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
            Ipld::Link(cid) => Self::CidLink(Cid::ipld(*cid)),
        })
    }
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
pub fn from_data<'de, T, S>(data: &'de Data<S>) -> Result<T, DataDeserializerError>
where
    T: serde::Deserialize<'de>,
    S: Bos<str> + AsRef<str> + serde::Deserialize<'de> + core::convert::From<&'de str>,
{
    T::deserialize(data)
}

/// Deserialize a typed value from a `Data` value
///
/// Takes ownership rather than borrows. Will allocate.
pub fn from_data_owned<'de, T, S>(
    data: Data<S>,
) -> Result<<T as IntoStatic>::Output, DataDeserializerError>
where
    T: serde::Deserialize<'de> + IntoStatic,
    S: Bos<str> + AsRef<str> + serde::Deserialize<'de> + IntoStatic + core::convert::From<&'de str>,
    <S as IntoStatic>::Output: Bos<str> + AsRef<str>,
{
    T::deserialize(data).map(|d| d.into_static())
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

/// Deserialize a typed value from cbor bytes
///
/// Returns an owned version, will allocate
pub fn from_cbor<'de, T>(
    cbor: &'de [u8],
) -> Result<<T as IntoStatic>::Output, serde_ipld_dagcbor::DecodeError<Infallible>>
where
    T: serde::Deserialize<'de> + IntoStatic,
{
    serde_ipld_dagcbor::from_slice::<T>(cbor).map(|d| d.into_static())
}

/// Deserialize a typed value from postcard bytes
///
/// Returns an owned version, will allocate
pub fn from_postcard<'de, T>(bytes: &'de [u8]) -> Result<<T as IntoStatic>::Output, postcard::Error>
where
    T: serde::Deserialize<'de> + IntoStatic,
{
    postcard::from_bytes::<T>(bytes).map(|d| d.into_static())
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
pub fn to_data<'s, T, S>(value: &T) -> Result<Data<S>, convert::ConversionError>
where
    T: serde::Serialize,
    S: Bos<str> + AsRef<str> + serde::Serialize + From<CowStr<'s>>,
{
    let raw = to_raw_data(value).map_err(|e| convert::ConversionError::InvalidRawData {
        message: e.to_string(),
    })?;
    raw.try_into()
}

/// Parse and traverse a path through nested Data structures
fn parse_and_traverse_path<'s, S>(data: &'s Data<S>, path: &str) -> Option<&'s Data<S>>
where
    S: AsRef<str> + Bos<str>,
{
    let mut current = data;
    let mut path = path.trim_start_matches('.');

    while !path.is_empty() {
        if path.starts_with('[') {
            // Array index: [N]
            let idx_end = path.find(']')?;
            let idx_str = &path[1..idx_end];
            let idx: usize = idx_str.parse().ok()?;

            current = current.as_array()?.get(idx)?;
            path = &path[idx_end + 1..].trim_start_matches('.');
        } else {
            // Field access: extract next segment (up to '.' or '[')
            let next_sep = path.find(&['.', '['][..]).unwrap_or(path.len());
            let field = &path[..next_sep];

            if field.is_empty() {
                break;
            }

            current = current.as_object()?.get(field)?;
            path = &path[next_sep..].trim_start_matches('.');
        }
    }

    Some(current)
}

/// Parse and traverse a path through nested RawData structures
fn parse_and_traverse_raw_path<'d>(data: &'d RawData<'d>, path: &str) -> Option<&'d RawData<'d>> {
    let mut current = data;
    let mut path = path.trim_start_matches('.');

    while !path.is_empty() {
        if path.starts_with('[') {
            // Array index: [N]
            let idx_end = path.find(']')?;
            let idx_str = &path[1..idx_end];
            let idx: usize = idx_str.parse().ok()?;

            current = current.as_array()?.get(idx)?;
            path = &path[idx_end + 1..].trim_start_matches('.');
        } else {
            // Field access: extract next segment (up to '.' or '[')
            let next_sep = path.find(&['.', '['][..]).unwrap_or(path.len());
            let field = &path[..next_sep];

            if field.is_empty() {
                break;
            }

            current = current.as_object()?.get(field as &str)?;
            path = &path[next_sep..].trim_start_matches('.');
        }
    }

    Some(current)
}

/// Parse and traverse a path through nested Data structures
fn parse_and_traverse_path_mut<'d, 's, S>(
    data: &'s mut Data<S>,
    path: &str,
) -> Option<&'s mut Data<S>>
where
    S: AsRef<str> + Bos<str>,
{
    let mut current = data;
    let mut path = path.trim_start_matches('.');

    while !path.is_empty() {
        if path.starts_with('[') {
            // Array index: [N]
            let idx_end = path.find(']')?;
            let idx_str = &path[1..idx_end];
            let idx: usize = idx_str.parse().ok()?;

            current = current.as_array_mut()?.get_mut(idx)?;
            path = &path[idx_end + 1..].trim_start_matches('.');
        } else {
            // Field access: extract next segment (up to '.' or '[')
            let next_sep = path.find(&['.', '['][..]).unwrap_or(path.len());
            let field = &path[..next_sep];

            if field.is_empty() {
                break;
            }

            current = current.as_object_mut()?.get_mut(field)?;
            path = &path[next_sep..].trim_start_matches('.');
        }
    }

    Some(current)
}

/// Parse and traverse a path through nested RawData structures
fn parse_and_traverse_raw_path_mut<'a, 'd>(
    data: &'a mut RawData<'d>,
    path: &str,
) -> Option<&'a mut RawData<'d>> {
    let mut current = data;
    let mut path = path.trim_start_matches('.');

    while !path.is_empty() {
        if path.starts_with('[') {
            // Array index: [N]
            let idx_end = path.find(']')?;
            let idx_str = &path[1..idx_end];
            let idx: usize = idx_str.parse().ok()?;

            current = current.as_array_mut()?.get_mut(idx)?;
            path = &path[idx_end + 1..].trim_start_matches('.');
        } else {
            // Field access: extract next segment (up to '.' or '[')
            let next_sep = path.find(&['.', '['][..]).unwrap_or(path.len());
            let field = &path[..next_sep];

            if field.is_empty() {
                break;
            }

            current = current.as_object_mut()?.get_mut(field as &str)?;
            path = &path[next_sep..].trim_start_matches('.');
        }
    }

    Some(current)
}

/// Result of a data query operation
#[derive(Debug, Clone, PartialEq)]
pub enum QueryResult<'s, S>
where
    S: AsRef<str> + Bos<str>,
{
    /// Single value expected and found
    Single(&'s Data<S>),

    /// Multiple values from wildcard or global recursion
    Multiple(Vec<QueryMatch<'s, S>>),

    /// No matches found
    None,
}

impl<'s, S> QueryResult<'s, S>
where
    S: AsRef<str> + Bos<str>,
{
    /// Get single value if available
    pub fn single(&self) -> Option<&'s Data<S>> {
        match self {
            QueryResult::Single(data) => Some(data),
            _ => None,
        }
    }

    /// Get multiple matches if available
    pub fn multiple(&self) -> Option<&[QueryMatch<'s, S>]> {
        match self {
            QueryResult::Multiple(matches) => Some(matches),
            _ => None,
        }
    }

    /// Get first value regardless of result type
    pub fn first(&self) -> Option<&'s Data<S>> {
        match self {
            QueryResult::Single(data) => Some(data),
            QueryResult::Multiple(matches) => matches.first().and_then(|m| m.value),
            QueryResult::None => None,
        }
    }

    /// Check if any results were found
    pub fn is_empty(&self) -> bool {
        matches!(self, QueryResult::None)
    }

    /// Get all values as an iterator (flattens single/multiple)
    pub fn values(&self) -> impl Iterator<Item = &'s Data<S>> {
        match self {
            QueryResult::Single(data) => vec![*data].into_iter(),
            QueryResult::Multiple(matches) => matches
                .iter()
                .filter_map(|m| m.value)
                .collect::<Vec<_>>()
                .into_iter(),
            QueryResult::None => vec![].into_iter(),
        }
    }
}

/// A single match from a query operation
#[derive(Debug, Clone, PartialEq)]
pub struct QueryMatch<'s, S>
where
    S: AsRef<str> + Bos<str>,
{
    /// Path where this value was found (e.g., "actors\[0\].handle")
    pub path: SmolStr,
    /// The value (None if field was missing during wildcard iteration)
    pub value: Option<&'s Data<S>>,
}

/// Query pattern segment
#[derive(Debug, Clone, PartialEq)]
enum QuerySegment {
    /// Exact field name
    Field(SmolStr),
    /// Wildcard [..]
    Wildcard,
    /// Scoped recursion ..field
    ScopedRecursion(SmolStr),
    /// Global recursion ...field
    GlobalRecursion(SmolStr),
}

/// Parse a query pattern into segments
fn parse_query_pattern(pattern: &str) -> Vec<QuerySegment> {
    let mut segments = Vec::new();
    let mut remaining = pattern;

    // Skip single leading dot if present
    if remaining.starts_with('.') && !remaining.starts_with("..") {
        remaining = &remaining[1..];
    }

    while !remaining.is_empty() {
        if remaining.starts_with("...") {
            // Global recursion
            let rest = &remaining[3..];
            let end = rest.find(&['.', '['][..]).unwrap_or(rest.len());
            let field = SmolStr::new(&rest[..end]);
            segments.push(QuerySegment::GlobalRecursion(field));
            remaining = &rest[end..];
            // Skip single dot separator
            if remaining.starts_with('.') && !remaining.starts_with("..") {
                remaining = &remaining[1..];
            }
        } else if remaining.starts_with("..") {
            // Scoped recursion
            let rest = &remaining[2..];
            let end = rest.find(&['.', '['][..]).unwrap_or(rest.len());
            let field = SmolStr::new(&rest[..end]);
            segments.push(QuerySegment::ScopedRecursion(field));
            remaining = &rest[end..];
            // Skip single dot separator
            if remaining.starts_with('.') && !remaining.starts_with("..") {
                remaining = &remaining[1..];
            }
        } else if remaining.starts_with("[..]") {
            // Wildcard
            segments.push(QuerySegment::Wildcard);
            remaining = &remaining[4..];
            // Skip single dot separator
            if remaining.starts_with('.') && !remaining.starts_with("..") {
                remaining = &remaining[1..];
            }
        } else {
            // Regular field
            let end = remaining.find(&['.', '['][..]).unwrap_or(remaining.len());
            let field = &remaining[..end];
            if !field.is_empty() {
                segments.push(QuerySegment::Field(SmolStr::new(field)));
            }
            remaining = &remaining[end..];
            // Skip single dot separator
            if remaining.starts_with('.') && !remaining.starts_with("..") {
                remaining = &remaining[1..];
            }
        }
    }

    segments
}

/// Execute a query on data
fn query_data<'s, S>(data: &'s Data<S>, pattern: &str) -> QueryResult<'s, S>
where
    S: AsRef<str> + Bos<str>,
{
    let segments = parse_query_pattern(pattern);
    if segments.is_empty() {
        return QueryResult::None;
    }

    let mut results = vec![QueryMatch {
        path: SmolStr::new_static(""),
        value: Some(data),
    }];

    // Determine result type based on segment types before consuming segments
    let has_wildcard = segments.iter().any(|s| matches!(s, QuerySegment::Wildcard));
    let has_global = segments
        .iter()
        .any(|s| matches!(s, QuerySegment::GlobalRecursion(_)));

    for segment in segments {
        results = execute_segment(&results, &segment);
        if results.is_empty() {
            return QueryResult::None;
        }
    }

    if has_wildcard || has_global || results.len() > 1 {
        QueryResult::Multiple(results)
    } else if results.len() == 1 {
        if let Some(value) = results[0].value {
            QueryResult::Single(value)
        } else {
            QueryResult::None
        }
    } else {
        QueryResult::None
    }
}

/// Execute a single segment on current results
fn execute_segment<'s, S>(
    current: &[QueryMatch<'s, S>],
    segment: &QuerySegment,
) -> Vec<QueryMatch<'s, S>>
where
    S: AsRef<str> + Bos<str>,
{
    let mut next = Vec::new();

    for qm in current {
        let Some(data) = qm.value else { continue };

        match segment {
            QuerySegment::Field(field) => {
                if let Some(obj) = data.as_object() {
                    if let Some(value) = obj.get(field.as_str()) {
                        let new_path = append_path(&qm.path, field.as_str());
                        next.push(QueryMatch {
                            path: new_path,
                            value: Some(value),
                        });
                    }
                }
            }

            QuerySegment::Wildcard => match data {
                Data::Array(arr) => {
                    for (idx, item) in arr.iter().enumerate() {
                        let new_path = append_path(&qm.path, &format!("[{}]", idx));
                        next.push(QueryMatch {
                            path: new_path,
                            value: Some(item),
                        });
                    }
                }
                Data::Object(obj) => {
                    for (key, value) in obj.iter() {
                        let new_path = append_path(&qm.path, key.as_str());
                        next.push(QueryMatch {
                            path: new_path,
                            value: Some(value),
                        });
                    }
                }
                _ => {}
            },

            QuerySegment::ScopedRecursion(field) => {
                if let Some(found) = find_field_recursive(data, field.as_str(), &qm.path) {
                    next.push(found);
                }
            }

            QuerySegment::GlobalRecursion(field) => {
                find_all_fields_recursive(data, field.as_str(), &qm.path, &mut next);
            }
        }
    }

    next
}

/// Recursively find first occurrence of a field (scoped recursion)
fn find_field_recursive<'s, S>(
    data: &'s Data<S>,
    field: &str,
    base_path: &SmolStr,
) -> Option<QueryMatch<'s, S>>
where
    S: AsRef<str> + Bos<str>,
{
    match data {
        Data::Object(obj) => {
            // Check direct children first
            if let Some(value) = obj.get(field) {
                let new_path = append_path(base_path, field);
                return Some(QueryMatch {
                    path: new_path,
                    value: Some(value),
                });
            }

            // Recurse into nested objects
            for (key, value) in obj.iter() {
                let new_path = append_path(base_path, key.as_str());
                if let Some(found) = find_field_recursive(value, field, &new_path) {
                    return Some(found);
                }
            }
        }
        Data::Array(arr) => {
            for (idx, item) in arr.iter().enumerate() {
                let new_path = append_path(base_path, &format!("[{}]", idx));
                if let Some(found) = find_field_recursive(item, field, &new_path) {
                    return Some(found);
                }
            }
        }
        _ => {}
    }

    None
}

/// Recursively find all occurrences of a field (global recursion)
fn find_all_fields_recursive<'s, S>(
    data: &'s Data<S>,
    field: &str,
    base_path: &SmolStr,
    results: &mut Vec<QueryMatch<'s, S>>,
) where
    S: AsRef<str> + Bos<str>,
{
    match data {
        Data::Object(obj) => {
            // Check direct children
            if let Some(value) = obj.get(field) {
                let new_path = append_path(base_path, field);
                results.push(QueryMatch {
                    path: new_path,
                    value: Some(value),
                });
            }

            // Recurse into all nested values
            for (key, value) in obj.iter() {
                let new_path = append_path(base_path, key.as_str());
                find_all_fields_recursive(value, field, &new_path, results);
            }
        }
        Data::Array(arr) => {
            for (idx, item) in arr.iter().enumerate() {
                let new_path = append_path(base_path, &format!("[{}]", idx));
                find_all_fields_recursive(item, field, &new_path, results);
            }
        }
        _ => {}
    }
}

/// Append a segment to a path
fn append_path(base: &SmolStr, segment: &str) -> SmolStr {
    if base.is_empty() {
        SmolStr::new(segment)
    } else if segment.starts_with('[') {
        SmolStr::new(format!("{}{}", base, segment))
    } else {
        SmolStr::new(format!("{}.{}", base, segment))
    }
}
