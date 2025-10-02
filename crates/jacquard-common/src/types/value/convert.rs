use crate::types::{
    DataModelType,
    cid::Cid,
    string::AtprotoStr,
    value::{Array, Data, Object},
};
use bytes::Bytes;
use core::{any::TypeId, fmt};
use smol_str::SmolStr;
use std::{borrow::ToOwned, boxed::Box, collections::BTreeMap, vec::Vec};

/// Error used for converting from and into [`crate::types::value::Data`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ConversionError {
    /// Error when the Atproto data type wasn't the one we expected.
    WrongAtprotoType {
        /// The expected type.
        expected: DataModelType,
        /// The actual type.
        found: DataModelType,
    },
    /// Error when the given Atproto data type cannot be converted into a certain value type.
    FromAtprotoData {
        /// The Atproto data type trying to convert from.
        from: DataModelType,
        /// The type trying to convert into.
        into: TypeId,
    },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::WrongAtprotoType { expected, found } => {
                write!(
                    formatter,
                    "kind error: expected {:?} but found {:?}",
                    expected, found
                )
            }
            Self::FromAtprotoData { from, into } => {
                write!(
                    formatter,
                    "conversion error: cannot convert {:?} into {:?}",
                    from, into
                )
            }
        }
    }
}

impl std::error::Error for ConversionError {}

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
