use std::{convert::Infallible, fmt, marker::PhantomData, ops::Deref, str::FromStr};

use compact_str::ToCompactString;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};

pub use cid::Cid as IpldCid;

use crate::CowStr;

/// raw
pub const ATP_CID_CODEC: u64 = 0x55;

/// SHA-256
pub const ATP_CID_HASH: u64 = 0x12;

/// base 32
pub const ATP_CID_BASE: multibase::Base = multibase::Base::Base32Lower;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Either the string form of a cid or the ipld form
/// For the IPLD form we also cache the string representation for later use.
///
/// Default on deserialization matches the format (if we get bytes, we try to decode)
pub enum Cid<'c> {
    Ipld { cid: IpldCid, s: CowStr<'c> },
    Str(CowStr<'c>),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    #[error("Invalid IPLD CID {:?}", 0)]
    Ipld(#[from] cid::Error),
    #[error("{:?}", 0)]
    Utf8(#[from] std::str::Utf8Error),
}

impl<'c> Cid<'c> {
    pub fn new(cid: &'c [u8]) -> Result<Self, Error> {
        if let Ok(cid) = IpldCid::try_from(cid.as_ref()) {
            Ok(Self::ipld(cid))
        } else {
            let cid_str = CowStr::from_utf8(cid)?;
            Ok(Self::Str(cid_str))
        }
    }
    pub fn ipld(cid: IpldCid) -> Self {
        let s = CowStr::Owned(
            cid.to_string_of_base(ATP_CID_BASE)
                .unwrap_or_default()
                .to_compact_string(),
        );
        Self::Ipld { cid, s }
    }

    pub fn str(cid: &'c str) -> Self {
        Self::Str(CowStr::Borrowed(cid))
    }

    pub fn cow_str(cid: CowStr<'c>) -> Self {
        Self::Str(cid)
    }

    pub fn to_ipld(&self) -> Result<IpldCid, cid::Error> {
        match self {
            Cid::Ipld { cid, s: _ } => Ok(cid.clone()),
            Cid::Str(cow_str) => IpldCid::try_from(cow_str.as_ref()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Cid::Ipld { cid: _, s } => s.as_ref(),
            Cid::Str(cow_str) => cow_str.as_ref(),
        }
    }
}

impl std::fmt::Display for Cid<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cid::Ipld { cid: _, s } => f.write_str(&s),
            Cid::Str(cow_str) => f.write_str(&cow_str),
        }
    }
}

impl FromStr for Cid<'_> {
    type Err = Infallible;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Cid::Str(CowStr::Owned(s.to_compact_string())))
    }
}

impl Serialize for Cid<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Cid::Ipld { cid, s: _ } => cid.serialize(serializer),
            Cid::Str(cow_str) => cow_str.serialize(serializer),
        }
    }
}

// TODO: take another look at this, see if we can do more borrowed and such
impl<'de> Deserialize<'de> for Cid<'_> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrBytes<T>(PhantomData<fn() -> T>);

        impl<'de, T> Visitor<'de> for StringOrBytes<T>
        where
            T: Deserialize<'de> + FromStr<Err = Infallible> + From<IpldCid>,
        {
            type Value = T;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("either valid IPLD CID bytes or a str")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(FromStr::from_str(v).unwrap())
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let hash = cid::multihash::Multihash::from_bytes(v).map_err(|e| E::custom(e))?;
                Ok(T::from(IpldCid::new_v1(ATP_CID_CODEC, hash)))
            }
        }

        deserializer.deserialize_any(StringOrBytes(PhantomData))
    }
}

impl From<Cid<'_>> for String {
    fn from(value: Cid) -> Self {
        let cow_str = match value {
            Cid::Ipld { cid: _, s } => s,
            Cid::Str(cow_str) => cow_str,
        };
        cow_str.to_string()
    }
}

impl<'d> From<Cid<'d>> for CowStr<'d> {
    fn from(value: Cid<'d>) -> Self {
        match value {
            Cid::Ipld { cid: _, s } => s,
            Cid::Str(cow_str) => cow_str,
        }
    }
}

impl From<String> for Cid<'_> {
    fn from(value: String) -> Self {
        Cid::Str(CowStr::Owned(value.to_compact_string()))
    }
}

impl<'d> From<CowStr<'d>> for Cid<'d> {
    fn from(value: CowStr<'d>) -> Self {
        Cid::Str(value)
    }
}

impl From<IpldCid> for Cid<'_> {
    fn from(value: IpldCid) -> Self {
        Cid::ipld(value)
    }
}

impl AsRef<str> for Cid<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Cid<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
