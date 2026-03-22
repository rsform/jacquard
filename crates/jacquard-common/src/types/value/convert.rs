use crate::types::cid::CidLink;
use crate::types::{
    DataModelType,
    cid::Cid,
    string::AtprotoStr,
    value::{Array, Data, Object, RawData, parsing},
};
use crate::{Bos, CowStr};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use bytes::Bytes;
use core::any::TypeId;
use core::convert::Infallible;
use core::str::FromStr;
use serde::Serialize;
use smol_str::SmolStr;
use std::borrow::Cow;

/// Error used for converting from and into [`crate::types::value::Data`].
#[derive(Clone, Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ConversionError {
    /// Error when the Atproto data type wasn't the one we expected.
    #[error("kind error: expected {expected:?} but found {found:?}")]
    WrongAtprotoType {
        /// The expected type.
        expected: DataModelType,
        /// The actual type.
        found: DataModelType,
    },
    /// Error when the given Atproto data type cannot be converted into a certain value type.
    #[error("conversion error: cannot convert {from:?} into {into:?}")]
    FromAtprotoData {
        /// The Atproto data type trying to convert from.
        from: DataModelType,
        /// The type trying to convert into.
        into: TypeId,
    },
    /// Error when converting from RawData containing invalid data
    #[error("invalid raw data: {message}")]
    InvalidRawData {
        /// Description of what was invalid
        message: String,
    },
}

impl<S> TryFrom<Data<S>> for ()
where
    S: AsRef<str> + Bos<str>,
{
    type Error = ConversionError;

    fn try_from(ipld: Data<S>) -> Result<Self, Self::Error> {
        match ipld {
            Data::Null => Ok(()),
            _ => Err(ConversionError::WrongAtprotoType {
                expected: DataModelType::Null,
                found: ipld.data_type(),
            }),
        }
    }
}

macro_rules! derive_try_from_atproto_option {
    ($enum:ident, $ty:ty) => {
        impl<S: 'static> TryFrom<Data<S>> for Option<$ty>
        where
            S: AsRef<str> + Bos<str>,
        {
            type Error = ConversionError;

            fn try_from(ipld: Data<S>) -> Result<Self, Self::Error> {
                match ipld {
                    Data::Null => Ok(None),
                    Data::$enum(value) => Ok(Some(value.try_into().map_err(|_| {
                        ConversionError::FromAtprotoData {
                            from: DataModelType::$enum,
                            into: TypeId::of::<$ty>(),
                        }
                    })?)),
                    _ => Err(ConversionError::WrongAtprotoType {
                        expected: DataModelType::$enum,
                        found: ipld.data_type(),
                    }),
                }
            }
        }
    };
}

macro_rules! derive_try_from_atproto {
    ($enum:ident, $ty:ty) => {
        impl<S: 'static> TryFrom<Data<S>> for $ty
        where
            S: AsRef<str> + Bos<str>,
        {
            type Error = ConversionError;

            fn try_from(ipld: Data<S>) -> Result<Self, Self::Error> {
                match ipld {
                    Data::$enum(value) => {
                        Ok(value
                            .try_into()
                            .map_err(|_| ConversionError::FromAtprotoData {
                                from: DataModelType::$enum,
                                into: TypeId::of::<$ty>(),
                            })?)
                    }

                    _ => Err(ConversionError::WrongAtprotoType {
                        expected: DataModelType::$enum,
                        found: ipld.data_type(),
                    }),
                }
            }
        }
    };
}

macro_rules! derive_into_atproto_prim {
    ($enum:ident, $ty:ty, $fn:ident) => {
        impl<S> From<$ty> for Data<S>
        where
            S: AsRef<str> + Bos<str>,
        {
            fn from(t: $ty) -> Self {
                Data::$enum(t.$fn() as _)
            }
        }
    };
}

macro_rules! derive_into_atproto {
    ($enum:ident, $ty:ty, $($fn:ident),*) => {
        impl<S> From<$ty> for Data<S>
        where
            S: AsRef<str> + Bos<str>, {
            fn from(t: $ty) -> Self {
                Data::$enum(t$(.$fn())*)
            }
        }
    };
}

impl<S> From<String> for Data<S>
where
    S: AsRef<str> + Bos<str> + From<String>,
{
    fn from(t: String) -> Self {
        Data::String(AtprotoStr::new(t.into()))
    }
}

impl<'a, S> From<&'a str> for Data<S>
where
    S: AsRef<str> + Bos<str> + From<&'a str>,
{
    fn from(t: &'a str) -> Self {
        Data::String(AtprotoStr::new(S::from(t)))
    }
}

impl<S> From<&[u8]> for Data<S>
where
    S: AsRef<str> + Bos<str>,
{
    fn from(t: &[u8]) -> Self {
        Data::Bytes(Bytes::copy_from_slice(t))
    }
}

impl<'s, S> From<CowStr<'s>> for Data<S>
where
    S: AsRef<str> + Bos<str> + FromStr<Err = Infallible>,
{
    fn from(t: CowStr<'s>) -> Self {
        Data::String(AtprotoStr::new(
            S::from_str(t.as_ref()).unwrap_or_else(|_| unreachable!()),
        ))
    }
}

impl<S> From<SmolStr> for Data<S>
where
    S: AsRef<str> + Bos<str> + From<SmolStr>,
{
    fn from(t: SmolStr) -> Self {
        Data::String(AtprotoStr::new(S::from(t)))
    }
}

impl<'s, S> From<Cow<'s, str>> for Data<S>
where
    S: AsRef<str> + Bos<str> + FromStr<Err = Infallible>,
{
    fn from(t: Cow<'s, str>) -> Self {
        Data::String(AtprotoStr::new(
            S::from_str(t.as_ref()).unwrap_or_else(|_| unreachable!()),
        ))
    }
}

impl<S> TryFrom<Data<S>> for Option<String>
where
    S: AsRef<str> + Bos<str> + Clone + Serialize,
{
    type Error = ConversionError;

    fn try_from(ipld: Data<S>) -> Result<Self, Self::Error> {
        match ipld {
            Data::Null => Ok(None),
            Data::String(value) => Ok(Some(value.try_into().map_err(|_| {
                ConversionError::FromAtprotoData {
                    from: DataModelType::String(crate::types::LexiconStringType::String),
                    into: TypeId::of::<String>(),
                }
            })?)),
            _ => Err(ConversionError::WrongAtprotoType {
                expected: DataModelType::String(crate::types::LexiconStringType::String),
                found: ipld.data_type(),
            }),
        }
    }
}

impl<S> TryFrom<Data<S>> for String
where
    S: AsRef<str> + Bos<str> + TryFrom<String> + Clone + Serialize,
{
    type Error = ConversionError;

    fn try_from(ipld: Data<S>) -> Result<Self, Self::Error> {
        match ipld {
            Data::String(value) => {
                Ok(value
                    .try_into()
                    .map_err(|_| ConversionError::FromAtprotoData {
                        from: DataModelType::String(crate::types::LexiconStringType::String),
                        into: TypeId::of::<String>(),
                    })?)
            }

            _ => Err(ConversionError::WrongAtprotoType {
                expected: DataModelType::String(crate::types::LexiconStringType::String),
                found: ipld.data_type(),
            }),
        }
    }
}

impl<S> From<Vec<Data<S>>> for Array<S>
where
    S: AsRef<str> + Bos<str>,
{
    fn from(value: Vec<Data<S>>) -> Self {
        Array(value)
    }
}

impl<S> From<BTreeMap<SmolStr, Data<S>>> for Object<S>
where
    S: AsRef<str> + Bos<str>,
{
    fn from(value: BTreeMap<SmolStr, Data<S>>) -> Self {
        Object(value)
    }
}

derive_into_atproto!(Boolean, bool, clone);
derive_into_atproto_prim!(Integer, i8, clone);
derive_into_atproto_prim!(Integer, i16, clone);
derive_into_atproto_prim!(Integer, i32, clone);
derive_into_atproto_prim!(Integer, i64, clone);
derive_into_atproto_prim!(Integer, i128, clone);
derive_into_atproto_prim!(Integer, isize, clone);
derive_into_atproto_prim!(Integer, u8, clone);
derive_into_atproto_prim!(Integer, u16, clone);
derive_into_atproto_prim!(Integer, u32, clone);
derive_into_atproto_prim!(Integer, u64, clone);
derive_into_atproto_prim!(Integer, usize, clone);
derive_into_atproto!(Bytes, Box<[u8]>, into);
derive_into_atproto!(Bytes, Vec<u8>, into);
derive_into_atproto!(Array, Array<S>,);
derive_into_atproto!(Object, Object<S>,);

derive_into_atproto!(CidLink, Cid<S>,);
derive_into_atproto!(Blob, crate::types::blob::Blob<S>,);
derive_into_atproto!(String, AtprotoStr<S>,);

impl<S> From<CidLink<S>> for Data<S>
where
    S: AsRef<str> + Bos<str>,
{
    fn from(t: CidLink<S>) -> Self {
        Data::CidLink(t.0)
    }
}

impl<S> From<crate::types::blob::BlobRef<S>> for Data<S>
where
    S: AsRef<str> + Bos<str>,
{
    fn from(t: crate::types::blob::BlobRef<S>) -> Self {
        Data::Blob(t.into())
    }
}

impl<S> From<&Cid<S>> for Data<S>
where
    S: AsRef<str> + Bos<str> + Clone,
{
    fn from(t: &Cid<S>) -> Self {
        Data::CidLink(t.clone())
    }
}

derive_try_from_atproto!(Boolean, bool);
derive_try_from_atproto!(Integer, i8);
derive_try_from_atproto!(Integer, i16);
derive_try_from_atproto!(Integer, i32);
derive_try_from_atproto!(Integer, i64);
derive_try_from_atproto!(Integer, i128);
derive_try_from_atproto!(Integer, isize);
derive_try_from_atproto!(Integer, u8);
derive_try_from_atproto!(Integer, u16);
derive_try_from_atproto!(Integer, u32);
derive_try_from_atproto!(Integer, u64);
derive_try_from_atproto!(Integer, u128);
derive_try_from_atproto!(Integer, usize);
derive_try_from_atproto!(Bytes, Vec<u8>);
derive_try_from_atproto!(Array, Array<S>);
derive_try_from_atproto!(Object, Object<S>);
derive_try_from_atproto!(CidLink, Cid<S>);
derive_try_from_atproto!(Blob, crate::types::blob::Blob<S>);

derive_try_from_atproto_option!(Boolean, bool);
derive_try_from_atproto_option!(Integer, i8);
derive_try_from_atproto_option!(Integer, i16);
derive_try_from_atproto_option!(Integer, i32);
derive_try_from_atproto_option!(Integer, i64);
derive_try_from_atproto_option!(Integer, i128);
derive_try_from_atproto_option!(Integer, isize);
derive_try_from_atproto_option!(Integer, u8);
derive_try_from_atproto_option!(Integer, u16);
derive_try_from_atproto_option!(Integer, u32);
derive_try_from_atproto_option!(Integer, u64);
derive_try_from_atproto_option!(Integer, u128);
derive_try_from_atproto_option!(Integer, usize);

derive_try_from_atproto_option!(Bytes, Vec<u8>);
derive_try_from_atproto_option!(Array, Array<S>);
derive_try_from_atproto_option!(Object, Object<S>);
derive_try_from_atproto_option!(CidLink, Cid<S>);
derive_try_from_atproto_option!(Blob, crate::types::blob::Blob<S>);

/// Convert RawData to validated Data with type inference
impl<'s, S> TryFrom<RawData<'s>> for Data<S>
where
    S: Bos<str> + AsRef<str> + From<CowStr<'s>>,
{
    type Error = ConversionError;

    fn try_from(raw: RawData<'s>) -> Result<Self, Self::Error> {
        match raw {
            RawData::Null => Ok(Data::Null),
            RawData::Boolean(b) => Ok(Data::Boolean(b)),
            RawData::SignedInt(i) => Ok(Data::Integer(i)),
            RawData::UnsignedInt(u) => {
                // Convert to i64, clamping if necessary
                Ok(Data::Integer((u % (i64::MAX as u64)) as i64))
            }
            RawData::String(s) => {
                // Apply string type inference
                Ok(Data::String(parsing::parse_string(S::from(s))))
            }
            RawData::Bytes(b) => Ok(Data::Bytes(b)),
            RawData::CidLink(cid) => Ok(Data::CidLink(cid.convert())),
            RawData::Array(arr) => {
                let mut validated = Vec::with_capacity(arr.len());
                for item in arr {
                    validated.push(item.try_into()?);
                }
                Ok(Data::Array(Array(validated)))
            }
            RawData::Object(map) => {
                // Check for special blob structure
                if let Some(RawData::String(type_str)) = map.get("$type") {
                    if parsing::infer_from_type(type_str) == DataModelType::Blob {
                        // Try to parse as blob
                        if let (
                            Some(RawData::CidLink(cid)),
                            Some(RawData::String(mime)),
                            Some(size),
                        ) = (map.get("ref"), map.get("mimeType"), map.get("size"))
                        {
                            let size_val = match size {
                                RawData::UnsignedInt(u) => *u as usize,
                                RawData::SignedInt(i) => *i as usize,
                                _ => {
                                    return Err(ConversionError::InvalidRawData {
                                        message: "blob size must be integer".to_string(),
                                    });
                                }
                            };
                            return Ok(Data::Blob(crate::types::blob::Blob {
                                r#ref: CidLink(cid.clone().convert()),
                                mime_type: crate::types::blob::MimeType::new(S::from(mime.clone())),
                                size: size_val,
                            }));
                        }
                    }
                }

                // Regular object - convert recursively with type inference based on keys
                let mut validated = BTreeMap::new();
                for (key, value) in map {
                    let data_value: Data<S> = value.try_into()?;
                    validated.insert(key, data_value);
                }
                Ok(Data::Object(Object(validated)))
            }
            RawData::Blob(blob) => Ok(Data::Blob(blob.convert())),
            RawData::InvalidBlob(_) => Err(ConversionError::InvalidRawData {
                message: "invalid blob structure".to_string(),
            }),
            RawData::InvalidNumber(_) => Err(ConversionError::InvalidRawData {
                message: "invalid number (likely float)".to_string(),
            }),
            RawData::InvalidData(_) => Err(ConversionError::InvalidRawData {
                message: "invalid data".to_string(),
            }),
        }
    }
}
