use crate::{CowStr, IntoStatic, types::cid::CidLink};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use smol_str::ToSmolStr;
use std::convert::Infallible;
use std::{fmt, hash::Hash, ops::Deref, str::FromStr};

/// Blob reference for binary data in AT Protocol
///
/// Blobs represent uploaded binary data (images, videos, etc.) stored separately from records.
/// They include a CID reference, MIME type, and size information.
///
/// Serialization differs between formats:
/// - JSON: `ref` is serialized as `{"$link": "cid_string"}`
/// - CBOR: `ref` is the raw CID
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Blob<'b> {
    /// CID (Content Identifier) reference to the blob data
    pub r#ref: CidLink<'b>,
    /// MIME type of the blob (e.g., "image/png", "video/mp4")
    #[serde(borrow)]
    pub mime_type: MimeType<'b>,
    /// Size of the blob in bytes
    pub size: usize,
}

impl Serialize for Blob<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        if serializer.is_human_readable() {
            // JSON: ref needs to be {"$link": "cid"}
            let mut map = serializer.serialize_map(Some(4))?;
            map.serialize_entry("$type", "blob")?;

            // Serialize ref as {"$link": "cid_string"}
            let mut ref_map = std::collections::BTreeMap::new();
            ref_map.insert("$link", self.r#ref.as_str());
            map.serialize_entry("ref", &ref_map)?;

            map.serialize_entry("mimeType", &self.mime_type)?;
            map.serialize_entry("size", &self.size)?;
            map.end()
        } else {
            // CBOR: ref is just the CID directly
            let mut map = serializer.serialize_map(Some(4))?;
            map.serialize_entry("$type", "blob")?;
            map.serialize_entry("ref", &self.r#ref)?;
            map.serialize_entry("mimeType", &self.mime_type)?;
            map.serialize_entry("size", &self.size)?;
            map.end()
        }
    }
}

impl IntoStatic for Blob<'_> {
    type Output = Blob<'static>;

    fn into_static(self) -> Self::Output {
        Blob {
            r#ref: self.r#ref.into_static(),
            mime_type: self.mime_type.into_static(),
            size: self.size,
        }
    }
}

/// Tagged blob reference with `$type` field for serde
///
/// This enum provides the `{"$type": "blob"}` wrapper expected by AT Protocol's JSON format.
/// Currently only contains the `Blob` variant, but the enum structure supports future extensions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type", rename_all = "lowercase")]
pub enum BlobRef<'r> {
    /// Blob variant with embedded blob data
    #[serde(borrow)]
    Blob(Blob<'r>),
}

impl<'r> BlobRef<'r> {
    /// Get the inner blob reference
    pub fn blob(&self) -> &Blob<'r> {
        match self {
            BlobRef::Blob(blob) => blob,
        }
    }
}

impl<'b> From<BlobRef<'b>> for Blob<'b> {
    fn from(blob_ref: BlobRef<'b>) -> Self {
        match blob_ref {
            BlobRef::Blob(blob) => blob,
        }
    }
}

impl<'b> From<Blob<'b>> for BlobRef<'b> {
    fn from(blob: Blob<'b>) -> Self {
        BlobRef::Blob(blob)
    }
}

impl IntoStatic for BlobRef<'_> {
    type Output = BlobRef<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            BlobRef::Blob(blob) => BlobRef::Blob(blob.into_static()),
        }
    }
}

/// MIME type identifier for blob data
///
/// Used to specify the content type of blobs. Supports patterns like "image/*" and "*/*".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct MimeType<'m>(pub CowStr<'m>);

impl<'m> MimeType<'m> {
    /// Fallible constructor, validates, borrows from input
    pub fn new(mime_type: &'m str) -> Result<MimeType<'m>, &'static str> {
        Ok(Self(CowStr::Borrowed(mime_type)))
    }

    /// Fallible constructor, validates, takes ownership
    pub fn new_owned(mime_type: impl AsRef<str>) -> Self {
        Self(CowStr::Owned(mime_type.as_ref().to_smolstr()))
    }

    /// Fallible constructor, validates, doesn't allocate
    pub fn new_static(mime_type: &'static str) -> Self {
        Self(CowStr::new_static(mime_type))
    }

    /// Fallible constructor from an existing CowStr
    pub fn from_cowstr(mime_type: CowStr<'m>) -> Result<MimeType<'m>, &'static str> {
        Ok(Self(mime_type))
    }

    /// Fallible constructor, validates, borrows from input if possible
    pub fn new_cow(mime_type: CowStr<'m>) -> Result<MimeType<'m>, &'static str> {
        Self::from_cowstr(mime_type)
    }

    /// Infallible constructor for trusted MIME type strings
    pub fn raw(mime_type: &'m str) -> Self {
        Self(CowStr::Borrowed(mime_type))
    }

    /// Get the MIME type as a string slice
    pub fn as_str(&self) -> &str {
        {
            let this = &self.0;
            this
        }
    }
}

impl FromStr for MimeType<'_> {
    type Err = Infallible;

    /// Has to take ownership due to the lifetime constraints of the FromStr trait.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new_owned(s))
    }
}

impl IntoStatic for MimeType<'_> {
    type Output = MimeType<'static>;

    fn into_static(self) -> Self::Output {
        MimeType(self.0.into_static())
    }
}

impl<'de, 'b> Deserialize<'de> for MimeType<'b>
where
    'de: 'b,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Deserialize::deserialize(deserializer)?;
        Self::new_cow(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for MimeType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'m> From<MimeType<'m>> for String {
    fn from(value: MimeType<'m>) -> Self {
        value.0.to_string()
    }
}

impl<'m> From<MimeType<'m>> for CowStr<'m> {
    fn from(value: MimeType<'m>) -> Self {
        value.0
    }
}

impl From<String> for MimeType<'static> {
    fn from(value: String) -> Self {
        Self(CowStr::Owned(value.to_smolstr()))
    }
}

impl<'m> From<CowStr<'m>> for MimeType<'m> {
    fn from(value: CowStr<'m>) -> Self {
        Self(value)
    }
}

impl AsRef<str> for MimeType<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for MimeType<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
