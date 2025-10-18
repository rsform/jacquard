use crate::IntoStatic;
use crate::types::cid::CidLink;
use crate::types::{
    DataModelType,
    cid::Cid,
    string::AtprotoStr,
    value::{Array, Data, Object, RawData, parsing},
};
use bytes::Bytes;
use core::any::TypeId;
use smol_str::SmolStr;
use std::{borrow::ToOwned, boxed::Box, collections::BTreeMap, vec::Vec};

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

impl TryFrom<Data<'_>> for () {
    type Error = ConversionError;

    fn try_from(ipld: Data) -> Result<Self, Self::Error> {
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
        impl TryFrom<Data<'static>> for Option<$ty> {
            type Error = ConversionError;

            fn try_from(ipld: Data<'static>) -> Result<Self, Self::Error> {
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
        impl TryFrom<Data<'static>> for $ty {
            type Error = ConversionError;

            fn try_from(ipld: Data<'static>) -> Result<Self, Self::Error> {
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
        impl<'s> From<$ty> for Data<'s> {
            fn from(t: $ty) -> Self {
                Data::$enum(t.$fn() as _)
            }
        }
    };
}

macro_rules! derive_into_atproto {
    ($enum:ident, $ty:ty, $($fn:ident),*) => {
        impl<'s> From<$ty> for Data<'s> {
            fn from(t: $ty) -> Self {
                Data::$enum(t$(.$fn())*)
            }
        }
    };
}

impl From<String> for Data<'_> {
    fn from(t: String) -> Self {
        Data::String(AtprotoStr::new_owned(t))
    }
}

impl From<&str> for Data<'_> {
    fn from(t: &str) -> Self {
        Data::String(AtprotoStr::new_owned(t))
    }
}

impl From<&[u8]> for Data<'_> {
    fn from(t: &[u8]) -> Self {
        Data::Bytes(Bytes::copy_from_slice(t))
    }
}

impl<'s> TryFrom<Data<'s>> for Option<String> {
    type Error = ConversionError;

    fn try_from(ipld: Data<'s>) -> Result<Self, Self::Error> {
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

impl<'s> TryFrom<Data<'s>> for String {
    type Error = ConversionError;

    fn try_from(ipld: Data<'s>) -> Result<Self, Self::Error> {
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

impl<'s> From<Vec<Data<'s>>> for Array<'s> {
    fn from(value: Vec<Data<'s>>) -> Self {
        Array(value)
    }
}

impl<'s> From<BTreeMap<SmolStr, Data<'s>>> for Object<'s> {
    fn from(value: BTreeMap<SmolStr, Data<'s>>) -> Self {
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
derive_into_atproto!(Array, Array<'s>, into);
derive_into_atproto!(Object, Object<'s>, to_owned);

derive_into_atproto!(CidLink, Cid<'s>, clone);
derive_into_atproto!(CidLink, &Cid<'s>, to_owned);

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
derive_try_from_atproto!(Object, Object<'static>);
derive_try_from_atproto!(CidLink, Cid<'static>);

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
derive_try_from_atproto_option!(Array, Array<'static>);
derive_try_from_atproto_option!(Object, Object<'static>);
derive_try_from_atproto_option!(CidLink, Cid<'static>);

/// Convert RawData to validated Data with type inference
impl<'s> TryFrom<RawData<'s>> for Data<'s> {
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
                // Need to convert to owned because parse_string borrows from its input
                Ok(Data::String(parsing::parse_string(&s).into_static()))
            }
            RawData::Bytes(b) => Ok(Data::Bytes(b)),
            RawData::CidLink(cid) => Ok(Data::CidLink(cid)),
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
                                r#ref: CidLink::str(cid).into_static(),
                                mime_type: crate::types::blob::MimeType::from(mime.clone()),
                                size: size_val,
                            }));
                        }
                    }
                }

                // Regular object - convert recursively with type inference based on keys
                let mut validated = BTreeMap::new();
                for (key, value) in map {
                    let data_value: Data = value.try_into()?;
                    validated.insert(key, data_value);
                }
                Ok(Data::Object(Object(validated)))
            }
            RawData::Blob(blob) => Ok(Data::Blob(blob)),
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
