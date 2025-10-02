use crate::{CowStr, IntoStatic};
pub use cid::Cid as IpldCid;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};
use smol_str::ToSmolStr;
use std::{convert::Infallible, fmt, marker::PhantomData, ops::Deref, str::FromStr};

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

    pub fn new_owned(cid: &[u8]) -> Result<Cid<'static>, Error> {
        if let Ok(cid) = IpldCid::try_from(cid.as_ref()) {
            Ok(Self::ipld(cid))
        } else {
            let cid_str = CowStr::from_utf8(cid)?;
            Ok(Cid::Str(cid_str.into_static()))
        }
    }

    pub fn ipld(cid: IpldCid) -> Cid<'static> {
        let s = CowStr::Owned(
            cid.to_string_of_base(ATP_CID_BASE)
                .unwrap_or_default()
                .to_smolstr(),
        );
        Cid::Ipld { cid, s }
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
        Ok(Cid::Str(CowStr::Owned(s.to_smolstr())))
    }
}

impl IntoStatic for Cid<'_> {
    type Output = Cid<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            Cid::Ipld { cid, s } => Cid::Ipld {
                cid,
                s: s.into_static(),
            },
            Cid::Str(cow_str) => Cid::Str(cow_str.into_static()),
        }
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
        Cid::Str(CowStr::Owned(value.to_smolstr()))
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

/// CID link wrapper that serializes as {"$link": "cid"} in JSON
/// and as raw CID in CBOR
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CidLink<'c>(pub Cid<'c>);

impl<'c> CidLink<'c> {
    pub fn new(cid: &'c [u8]) -> Result<Self, Error> {
        Ok(Self(Cid::new(cid)?))
    }

    pub fn new_owned(cid: &[u8]) -> Result<CidLink<'static>, Error> {
        Ok(CidLink(Cid::new_owned(cid)?))
    }

    pub fn new_static(cid: &'static str) -> Self {
        Self(Cid::str(cid))
    }

    pub fn ipld(cid: IpldCid) -> CidLink<'static> {
        CidLink(Cid::ipld(cid))
    }

    pub fn str(cid: &'c str) -> Self {
        Self(Cid::str(cid))
    }

    pub fn cow_str(cid: CowStr<'c>) -> Self {
        Self(Cid::cow_str(cid))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn to_ipld(&self) -> Result<IpldCid, cid::Error> {
        self.0.to_ipld()
    }

    pub fn into_inner(self) -> Cid<'c> {
        self.0
    }
}

impl fmt::Display for CidLink<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for CidLink<'_> {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(CidLink(Cid::from_str(s)?))
    }
}

impl IntoStatic for CidLink<'_> {
    type Output = CidLink<'static>;

    fn into_static(self) -> Self::Output {
        CidLink(self.0.into_static())
    }
}

impl Serialize for CidLink<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // JSON: {"$link": "cid_string"}
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("$link", self.0.as_str())?;
            map.end()
        } else {
            // CBOR: raw CID
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for CidLink<'_> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            // JSON: expect {"$link": "cid_string"}
            struct LinkVisitor;

            impl<'de> Visitor<'de> for LinkVisitor {
                type Value = CidLink<'static>;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("a CID link object with $link field")
                }

                fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::MapAccess<'de>,
                {
                    use serde::de::Error;

                    let mut link: Option<String> = None;

                    while let Some(key) = map.next_key::<String>()? {
                        if key == "$link" {
                            link = Some(map.next_value()?);
                        } else {
                            // Skip unknown fields
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }

                    if let Some(cid_str) = link {
                        Ok(CidLink(Cid::from(cid_str)))
                    } else {
                        Err(A::Error::missing_field("$link"))
                    }
                }
            }

            deserializer.deserialize_map(LinkVisitor)
        } else {
            // CBOR: raw CID
            Ok(CidLink(Cid::deserialize(deserializer)?))
        }
    }
}

impl From<CidLink<'_>> for String {
    fn from(value: CidLink) -> Self {
        value.0.into()
    }
}

impl<'c> From<CidLink<'c>> for CowStr<'c> {
    fn from(value: CidLink<'c>) -> Self {
        value.0.into()
    }
}

impl From<String> for CidLink<'_> {
    fn from(value: String) -> Self {
        CidLink(Cid::from(value))
    }
}

impl<'c> From<CowStr<'c>> for CidLink<'c> {
    fn from(value: CowStr<'c>) -> Self {
        CidLink(Cid::from(value))
    }
}

impl From<IpldCid> for CidLink<'_> {
    fn from(value: IpldCid) -> Self {
        CidLink(Cid::from(value))
    }
}

impl<'c> From<Cid<'c>> for CidLink<'c> {
    fn from(value: Cid<'c>) -> Self {
        CidLink(value)
    }
}

impl<'c> From<CidLink<'c>> for Cid<'c> {
    fn from(value: CidLink<'c>) -> Self {
        value.0
    }
}

impl AsRef<str> for CidLink<'_> {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Deref for CidLink<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CID: &str = "bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha";

    #[test]
    fn cidlink_serialize_json() {
        let link = CidLink::str(TEST_CID);
        let json = serde_json::to_string(&link).unwrap();
        assert_eq!(
            json,
            r#"{"$link":"bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"}"#
        );
    }

    #[test]
    fn cidlink_deserialize_json() {
        let json = r#"{"$link":"bafyreih4g7bvo6hdq2juolev5bfzpbo4ewkxh5mzxwgvkjp3kitc6hqkha"}"#;
        let link: CidLink = serde_json::from_str(json).unwrap();
        assert_eq!(link.as_str(), TEST_CID);
    }

    #[test]
    fn cidlink_roundtrip_json() {
        let link = CidLink::str(TEST_CID);
        let json = serde_json::to_string(&link).unwrap();
        let parsed: CidLink = serde_json::from_str(&json).unwrap();
        assert_eq!(link, parsed);
        assert_eq!(link.as_str(), TEST_CID);
    }

    #[test]
    fn cidlink_constructors() {
        let link1 = CidLink::str(TEST_CID);
        let link2 = CidLink::cow_str(CowStr::Borrowed(TEST_CID));
        let link3 = CidLink::from(TEST_CID.to_string());
        let link4 = CidLink::new_static(TEST_CID);

        assert_eq!(link1.as_str(), TEST_CID);
        assert_eq!(link2.as_str(), TEST_CID);
        assert_eq!(link3.as_str(), TEST_CID);
        assert_eq!(link4.as_str(), TEST_CID);
    }

    #[test]
    fn cidlink_conversions() {
        let link = CidLink::str(TEST_CID);

        // CidLink -> Cid
        let cid: Cid = link.clone().into();
        assert_eq!(cid.as_str(), TEST_CID);

        // Cid -> CidLink
        let link2: CidLink = cid.into();
        assert_eq!(link2.as_str(), TEST_CID);

        // CidLink -> String
        let s: String = link.clone().into();
        assert_eq!(s, TEST_CID);

        // CidLink -> CowStr
        let cow: CowStr = link.into();
        assert_eq!(cow.as_ref(), TEST_CID);
    }

    #[test]
    fn cidlink_display() {
        let link = CidLink::str(TEST_CID);
        assert_eq!(format!("{}", link), TEST_CID);
    }

    #[test]
    fn cidlink_deref() {
        let link = CidLink::str(TEST_CID);
        assert_eq!(&*link, TEST_CID);
        assert_eq!(link.as_ref(), TEST_CID);
    }
}
