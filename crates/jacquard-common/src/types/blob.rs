use crate::bos::{Bos, DefaultStr};
use crate::types::cid::Cid;
use crate::{CowStr, IntoStatic, types::cid::CidLink};
use alloc::string::{String, ToString};
use core::convert::Infallible;
use core::{fmt, hash::Hash, ops::Deref, str::FromStr};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use smol_str::{SmolStr, ToSmolStr};

/// Blob reference for binary data in AT Protocol.
///
/// Serialization differs between formats:
/// - JSON: `ref` is serialized as `{"$link": "cid_string"}`
/// - CBOR: `ref` is the raw CID
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
#[serde(bound(deserialize = "S: Bos<str> + AsRef<str> + Deserialize<'de>"))]
pub struct Blob<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// CID (Content Identifier) reference to the blob data.
    pub r#ref: CidLink<S>,
    /// MIME type of the blob (e.g., "image/png", "video/mp4").
    pub mime_type: MimeType<S>,
    /// Size of the blob in bytes.
    pub size: usize,
}

impl<S: Bos<str> + AsRef<str>> Blob<S> {
    /// Get the inner CID reference.
    pub fn cid(&self) -> &Cid<S> {
        &self.r#ref.0
    }
}

impl<S: Bos<str> + AsRef<str> + Serialize> Serialize for Blob<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        use serde::ser::SerializeMap;

        if serializer.is_human_readable() {
            // JSON: ref needs to be {"$link": "cid"}.
            let mut map = serializer.serialize_map(Some(4))?;
            map.serialize_entry("$type", "blob")?;

            // Serialize ref as {"$link": "cid_string"}.
            let mut ref_map = alloc::collections::BTreeMap::new();
            ref_map.insert("$link", self.r#ref.as_str());
            map.serialize_entry("ref", &ref_map)?;

            map.serialize_entry("mimeType", &self.mime_type)?;
            map.serialize_entry("size", &self.size)?;
            map.end()
        } else {
            // CBOR: ref is just the CID directly.
            let mut map = serializer.serialize_map(Some(4))?;
            map.serialize_entry("$type", "blob")?;
            map.serialize_entry("ref", &self.r#ref)?;
            map.serialize_entry("mimeType", &self.mime_type)?;
            map.serialize_entry("size", &self.size)?;
            map.end()
        }
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for Blob<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = Blob<S::Output>;

    fn into_static(self) -> Self::Output {
        Blob {
            r#ref: self.r#ref.into_static(),
            mime_type: self.mime_type.into_static(),
            size: self.size,
        }
    }
}

/// Tagged blob reference with `$type` field for serde.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type", rename_all = "lowercase")]
#[serde(bound(
    serialize = "S: Bos<str> + AsRef<str> + Serialize",
    deserialize = "S: Bos<str> + AsRef<str> + Deserialize<'de>"
))]
pub enum BlobRef<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// Blob variant with embedded blob data.
    Blob(Blob<S>),
}

impl<S: Bos<str> + AsRef<str>> BlobRef<S> {
    /// Get the inner blob reference.
    pub fn blob(&self) -> &Blob<S> {
        match self {
            BlobRef::Blob(blob) => blob,
        }
    }
}

impl<S: Bos<str> + AsRef<str>> From<BlobRef<S>> for Blob<S> {
    fn from(blob_ref: BlobRef<S>) -> Self {
        match blob_ref {
            BlobRef::Blob(blob) => blob,
        }
    }
}

impl<S: Bos<str> + AsRef<str>> From<Blob<S>> for BlobRef<S> {
    fn from(blob: Blob<S>) -> Self {
        BlobRef::Blob(blob)
    }
}

impl<S: Bos<str> + AsRef<str> + IntoStatic> IntoStatic for BlobRef<S>
where
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = BlobRef<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            BlobRef::Blob(blob) => BlobRef::Blob(blob.into_static()),
        }
    }
}

/// MIME type identifier for blob data.
///
/// Used to specify the content type of blobs. Supports patterns like "image/*" and "*/*".
/// No validation is performed — any string is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct MimeType<S: Bos<str> = DefaultStr>(pub S);

impl<S: Bos<str> + AsRef<str>> MimeType<S> {
    /// Get the MIME type as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl<'m> MimeType<&'m str> {
    /// Infallible constructor, borrows from input.
    pub fn new(mime_type: &'m str) -> Self {
        Self(mime_type)
    }

    /// Infallible constructor for trusted MIME type strings.
    pub fn raw(mime_type: &'m str) -> Self {
        Self(mime_type)
    }
}

impl<S: Bos<str> + From<SmolStr>> MimeType<S> {
    /// Infallible constructor, takes ownership.
    pub fn new_owned(mime_type: impl AsRef<str>) -> Self {
        Self(S::from(mime_type.as_ref().to_smolstr()))
    }

    /// Infallible constructor for static strings.
    pub fn new_static(mime_type: &'static str) -> Self {
        Self(S::from(SmolStr::new_static(mime_type)))
    }
}

impl<'m> MimeType<CowStr<'m>> {
    /// Infallible constructor, borrows if possible.
    pub fn new_cow(mime_type: CowStr<'m>) -> Self {
        Self(mime_type)
    }
}

impl<'de, S> Deserialize<'de> for MimeType<S>
where
    S: Bos<str> + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(MimeType(S::deserialize(deserializer)?))
    }
}

impl FromStr for MimeType {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new_owned(s))
    }
}

impl<S: Bos<str> + IntoStatic> IntoStatic for MimeType<S>
where
    S::Output: Bos<str>,
{
    type Output = MimeType<S::Output>;

    fn into_static(self) -> Self::Output {
        MimeType(self.0.into_static())
    }
}

impl<S: Bos<str> + AsRef<str>> fmt::Display for MimeType<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<S: Bos<str> + AsRef<str>> From<MimeType<S>> for String {
    fn from(value: MimeType<S>) -> Self {
        value.as_str().to_string()
    }
}

impl From<String> for MimeType {
    fn from(value: String) -> Self {
        Self::new_owned(value)
    }
}

impl<'m> From<CowStr<'m>> for MimeType<CowStr<'m>> {
    fn from(value: CowStr<'m>) -> Self {
        Self(value)
    }
}

impl<S: Bos<str> + AsRef<str>> AsRef<str> for MimeType<S> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<S: Bos<str> + AsRef<str>> Deref for MimeType<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
