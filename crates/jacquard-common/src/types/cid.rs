use crate::bos::{Bos, DefaultStr};
use crate::{CowStr, IntoStatic};
use alloc::string::{String, ToString};
pub use cid::Cid as IpldCid;
use core::{convert::Infallible, fmt, ops::Deref, str::FromStr};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};
use smol_str::{SmolStr, ToSmolStr};

/// CID codec for AT Protocol (raw).
pub const ATP_CID_CODEC: u64 = 0x55;

/// CID hash function for AT Protocol (SHA-256).
pub const ATP_CID_HASH: u64 = 0x12;

/// CID encoding base for AT Protocol (base32 lowercase).
pub const ATP_CID_BASE: multibase::Base = multibase::Base::Base32Lower;

/// Content Identifier (CID) for IPLD data in AT Protocol.
///
/// CIDs are self-describing content addresses used to reference IPLD data.
/// This type supports both string and parsed IPLD forms, with string caching
/// for the parsed form to optimise serialization.
///
/// # Validation
///
/// String deserialization does NOT validate CIDs. This is intentional for performance:
/// CID strings from AT Protocol endpoints are generally trustworthy, so validation
/// is deferred until needed. Use `to_ipld()` to parse and validate, or `is_valid()`
/// to check without parsing.
///
/// Byte deserialization (CBOR) parses immediately since the data is already in binary form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Cid<S: Bos<str> = DefaultStr> {
    /// Parsed IPLD CID with cached string representation.
    /// The cached string is always SmolStr regardless of `S`.
    Ipld {
        /// Parsed CID structure.
        cid: IpldCid,
        /// Cached base32 string form.
        s: SmolStr,
    },
    /// String-only form (not yet parsed).
    Str(S),
}

/// Errors that can occur when working with CIDs.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum Error {
    /// Invalid IPLD CID structure.
    #[error("Invalid IPLD CID {:?}", 0)]
    Ipld(#[from] cid::Error),
    /// Invalid UTF-8 in CID string.
    #[error("{:?}", 0)]
    Utf8(#[from] core::str::Utf8Error),
    /// Wraps another error with additional context
    #[error("converting from a string slice")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::cid::str_conversion)))]
    Conversion,
}

// ---------------------------------------------------------------------------
// Core methods
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Cid<S> {
    /// Get the CID as a string slice.
    pub fn as_str(&self) -> &str {
        match self {
            Cid::Ipld { cid: _, s } => s.as_ref(),
            Cid::Str(s) => s.as_ref(),
        }
    }

    /// Convert to a parsed IPLD CID (parses if needed).
    pub fn to_ipld(&self) -> Result<IpldCid, cid::Error> {
        match self {
            Cid::Ipld { cid, s: _ } => Ok(cid.clone()),
            Cid::Str(s) => IpldCid::try_from(s.as_ref()),
        }
    }

    /// Check if the CID string is valid without parsing.
    ///
    /// Returns `true` if the CID is already parsed (`Ipld` variant) or if
    /// the string can be successfully parsed as an IPLD CID.
    pub fn is_valid(&self) -> bool {
        match self {
            Cid::Ipld { .. } => true,
            Cid::Str(s) => IpldCid::try_from(s.as_ref()).is_ok(),
        }
    }
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl<S: Bos<str>> Cid<S> {
    /// Wrap a string directly as a CID without validation.
    ///
    /// # Safety
    ///
    /// The caller must ensure the string is a valid CID.
    pub unsafe fn unchecked_str(s: S) -> Self {
        Cid::Str(s)
    }

    /// Construct a CID from a parsed IPLD CID.
    pub fn ipld(cid: IpldCid) -> Self {
        let s = cid
            .to_string_of_base(ATP_CID_BASE)
            .unwrap_or_default()
            .to_smolstr();
        Cid::Ipld { cid, s }
    }
}

impl<'c> Cid<&'c str> {
    /// Construct a CID from a string slice (borrows).
    pub fn str(cid: &'c str) -> Self {
        Self::Str(cid)
    }
}

impl<S: Bos<str>> Cid<S> {
    /// Parse a CID from bytes (tries IPLD first, falls back to UTF-8 string).
    pub fn new<'c>(cid: &'c [u8]) -> Result<Cid<S>, Error>
    where
        S: From<&'c str>,
    {
        if let Ok(cid) = IpldCid::try_from(cid.as_ref()) {
            Ok(Cid::ipld(cid))
        } else {
            let cid_str = core::str::from_utf8(cid)?;
            Ok(Cid::Str(cid_str.into()))
        }
    }
}

impl<S: Bos<str> + FromStr> Cid<S> {
    /// Parse a CID from bytes into an owned value.
    pub fn new_owned(cid: &[u8]) -> Result<Self, Error> {
        if let Ok(cid) = IpldCid::try_from(cid.as_ref()) {
            Ok(Self::ipld(cid))
        } else {
            let cid_str = core::str::from_utf8(cid)?;
            Ok(Cid::Str(
                S::from_str(cid_str).map_err(|_| Error::Conversion)?,
            ))
        }
    }
}

impl<'c> Cid<CowStr<'c>> {
    /// Construct a CID from a CowStr.
    pub fn cow_str(cid: CowStr<'c>) -> Self {
        Self::Str(cid)
    }
}

// ---------------------------------------------------------------------------
// Serialization — preserving existing logic exactly
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Serialize for Cid<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        if serializer.is_human_readable() {
            self.as_str().serialize(serializer)
        } else {
            self.to_ipld()
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer)
        }
    }
}

// ---------------------------------------------------------------------------
// Deserialization — preserving existing logic exactly
// ---------------------------------------------------------------------------

impl<'de, S> Deserialize<'de> for Cid<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            // JSON: deserialize S (string), wrap in Str variant.
            let s = S::deserialize(deserializer)?;
            Ok(Cid::Str(s))
        } else {
            // CBOR/postcard: use IpldCid's deserializer for canonical CID bytes.
            let cid = IpldCid::deserialize(deserializer)?;
            Ok(Cid::ipld(cid))
        }
    }
}

// ---------------------------------------------------------------------------
// Trait impls
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> fmt::Display for Cid<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Cid::Ipld { cid: _, s } => f.write_str(s),
            Cid::Str(s) => f.write_str(s.as_ref()),
        }
    }
}

impl FromStr for Cid {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Cid::Str(s.to_smolstr()))
    }
}

impl<S: Bos<str> + IntoStatic> IntoStatic for Cid<S>
where
    S::Output: Bos<str>,
{
    type Output = Cid<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            Cid::Ipld { cid, s } => Cid::Ipld { cid, s },
            Cid::Str(s) => Cid::Str(s.into_static()),
        }
    }
}

impl<S: Bos<str>> Cid<S> {
    /// Convert to a `Cid` with a different backing type.
    pub fn convert<B: Bos<str> + From<S>>(self) -> Cid<B> {
        match self {
            Cid::Ipld { cid, s } => Cid::Ipld { cid, s },
            Cid::Str(s) => Cid::Str(B::from(s)),
        }
    }
}

impl<S: Bos<str> + AsRef<str>> From<Cid<S>> for String {
    fn from(value: Cid<S>) -> Self {
        value.as_str().to_string()
    }
}

impl From<String> for Cid {
    fn from(value: String) -> Self {
        Cid::Str(value.to_smolstr())
    }
}

impl<'d> From<CowStr<'d>> for Cid<CowStr<'d>> {
    fn from(value: CowStr<'d>) -> Self {
        Cid::Str(value)
    }
}

impl<S: Bos<str>> From<IpldCid> for Cid<S> {
    fn from(value: IpldCid) -> Self {
        Cid::ipld(value)
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for Cid<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for Cid<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

// ===========================================================================
// CidLink
// ===========================================================================

/// CID link wrapper for JSON `{"$link": "cid"}` serialization.
///
/// Wraps a `Cid` and handles format-specific serialization:
/// - JSON: `{"$link": "cid_string"}`
/// - CBOR: raw CID bytes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CidLink<S: Bos<str> = DefaultStr>(pub Cid<S>);

impl<S: Bos<str> + AsRef<str>> CidLink<S> {
    /// Get the CID as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Convert to a parsed IPLD CID.
    pub fn to_ipld(&self) -> Result<IpldCid, cid::Error> {
        self.0.to_ipld()
    }

    /// Unwrap into the inner Cid.
    pub fn into_inner(self) -> Cid<S> {
        self.0
    }
}

impl<S: Bos<str>> CidLink<S> {
    /// Construct a CID link from a parsed IPLD CID.
    pub fn ipld(cid: IpldCid) -> Self {
        CidLink(Cid::ipld(cid))
    }

    /// Parse a CID link from bytes.
    pub fn new<'c>(cid: &'c [u8]) -> Result<CidLink<S>, Error>
    where
        S: Bos<str> + From<&'c str>,
    {
        Ok(CidLink(Cid::new(cid)?))
    }
}

impl<'c> CidLink<&'c str> {
    /// Construct a CID link from a string slice.
    pub fn str(cid: &'c str) -> Self {
        Self(Cid::str(cid))
    }

    /// Construct a CID link from a static string.
    pub fn new_static(cid: &'static str) -> Self {
        Self(Cid::str(cid))
    }
}

impl<S: Bos<str> + FromStr> CidLink<S> {
    /// Parse a CID link from bytes into an owned value.
    pub fn new_owned(cid: &[u8]) -> Result<Self, Error> {
        Ok(CidLink(Cid::new_owned(cid)?))
    }
}

impl<'c> CidLink<CowStr<'c>> {
    /// Construct a CID link from a CowStr.
    pub fn cow_str(cid: CowStr<'c>) -> Self {
        Self(Cid::cow_str(cid))
    }
}

// ---------------------------------------------------------------------------
// CidLink serialization — preserving existing logic exactly
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> Serialize for CidLink<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
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

// ---------------------------------------------------------------------------
// CidLink deserialization — preserving existing logic exactly
// ---------------------------------------------------------------------------

impl<'de, S> Deserialize<'de> for CidLink<S>
where
    S: Bos<str> + AsRef<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use core::marker::PhantomData;

        if deserializer.is_human_readable() {
            // JSON: expect {"$link": "cid_string"}
            struct LinkVisitor<S>(PhantomData<fn() -> S>);

            impl<'de, S> Visitor<'de> for LinkVisitor<S>
            where
                S: Bos<str> + AsRef<str> + Deserialize<'de>,
            {
                type Value = CidLink<S>;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("a CID link object with $link field")
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    // TODO: currently overly permissive, should fix.
                    // Delegate to S's Deserialize via a StrDeserializer.
                    let s = S::deserialize(serde::de::value::StrDeserializer::<E>::new(v))?;
                    Ok(CidLink(Cid::Str(s)))
                }

                fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    // Binary CID data — parse as IpldCid (produces Ipld variant with SmolStr).
                    let cid = IpldCid::try_from(v).map_err(E::custom)?;
                    Ok(CidLink(Cid::ipld(cid)))
                }

                fn visit_byte_buf<E>(self, v: alloc::vec::Vec<u8>) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    self.visit_bytes(&v)
                }

                fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
                where
                    D: serde::de::Deserializer<'de>,
                {
                    // serde_ipld_dagcbor buffers CBOR tag-42 CIDs as a newtype struct wrapping
                    // raw CID bytes when deserializing through internally-tagged enums (Content).
                    // Must use deserialize_bytes (not Vec<u8>'s deserialize_seq) to avoid
                    // "byte array, expected a sequence" from ContentDeserializer.
                    struct BytesVisitor;
                    impl<'de> Visitor<'de> for BytesVisitor {
                        type Value = alloc::vec::Vec<u8>;
                        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                            f.write_str("CID bytes")
                        }
                        fn visit_bytes<E: serde::de::Error>(
                            self,
                            v: &[u8],
                        ) -> Result<Self::Value, E> {
                            Ok(v.to_vec())
                        }
                        fn visit_byte_buf<E: serde::de::Error>(
                            self,
                            v: alloc::vec::Vec<u8>,
                        ) -> Result<Self::Value, E> {
                            Ok(v)
                        }
                    }
                    let bytes = deserializer.deserialize_bytes(BytesVisitor)?;
                    self.visit_byte_buf(bytes)
                }

                fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::MapAccess<'de>,
                {
                    use serde::de::Error;

                    // Deserialize the $link value as Cid<S>, which delegates
                    // to S::deserialize for the string content.
                    let mut link: Option<Cid<S>> = None;

                    while let Some(key) = map.next_key::<String>()? {
                        if key == "$link" {
                            link = Some(map.next_value()?);
                        } else {
                            // Skip unknown fields.
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }

                    if let Some(cid) = link {
                        Ok(CidLink(cid))
                    } else {
                        Err(A::Error::missing_field("$link"))
                    }
                }
            }

            deserializer.deserialize_any(LinkVisitor(PhantomData))
        } else {
            // CBOR: raw CID
            Ok(CidLink(Cid::deserialize(deserializer)?))
        }
    }
}

// ---------------------------------------------------------------------------
// CidLink trait impls
// ---------------------------------------------------------------------------

impl<S: Bos<str> + AsRef<str>> fmt::Display for CidLink<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for CidLink {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(CidLink(Cid::from_str(s)?))
    }
}

impl<S: Bos<str> + IntoStatic> IntoStatic for CidLink<S>
where
    S::Output: Bos<str>,
{
    type Output = CidLink<S::Output>;

    fn into_static(self) -> Self::Output {
        CidLink(self.0.into_static())
    }
}

impl<S: Bos<str>> CidLink<S> {
    /// Convert to a `CidLink` with a different backing type.
    pub fn convert<B: Bos<str> + From<S>>(self) -> CidLink<B> {
        CidLink(self.0.convert())
    }
}

impl<S: Bos<str> + AsRef<str>> From<CidLink<S>> for String {
    fn from(value: CidLink<S>) -> Self {
        value.0.into()
    }
}

impl From<String> for CidLink {
    fn from(value: String) -> Self {
        CidLink(Cid::from(value))
    }
}

impl<'c> From<CowStr<'c>> for CidLink<CowStr<'c>> {
    fn from(value: CowStr<'c>) -> Self {
        CidLink(Cid::from(value))
    }
}

impl<S: Bos<str>> From<IpldCid> for CidLink<S> {
    fn from(value: IpldCid) -> Self {
        CidLink(Cid::from(value))
    }
}

impl<S: Bos<str>> From<Cid<S>> for CidLink<S> {
    fn from(value: Cid<S>) -> Self {
        CidLink(value)
    }
}

impl<S: Bos<str>> From<CidLink<S>> for Cid<S> {
    fn from(value: CidLink<S>) -> Self {
        value.0
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for CidLink<S> {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for CidLink<S> {
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
        assert_eq!(link.as_str(), parsed.as_str());
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
        let link = CidLink::<SmolStr>::from(TEST_CID.to_string());

        // CidLink -> Cid
        let cid: Cid<SmolStr> = link.clone().into();
        assert_eq!(cid.as_str(), TEST_CID);

        // Cid -> CidLink
        let link2: CidLink<SmolStr> = cid.into();
        assert_eq!(link2.as_str(), TEST_CID);

        // CidLink -> String
        let s: String = link.clone().into();
        assert_eq!(s, TEST_CID);
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

    #[test]
    fn cid_string_roundtrips_through_postcard_as_binary_cid() {
        let cid = Cid::<SmolStr>::from(TEST_CID.to_string());
        let bytes = postcard::to_allocvec(&cid).unwrap();
        let parsed: Cid<SmolStr> = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.as_str(), TEST_CID);
        assert!(matches!(parsed, Cid::Ipld { .. }));
    }

    #[test]
    fn cid_string_roundtrips_through_dag_cbor_as_binary_cid() {
        let cid = Cid::<SmolStr>::from(TEST_CID.to_string());
        let bytes = serde_ipld_dagcbor::to_vec(&cid).unwrap();
        let parsed: Cid<SmolStr> = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        assert_eq!(parsed.as_str(), TEST_CID);
        assert!(matches!(parsed, Cid::Ipld { .. }));
    }
}
