use serde::{Deserialize, Serialize};

/// AT Protocol URI (at://) types and validation
pub mod aturi;
/// Blob references for binary data
pub mod blob;
/// Content Identifier (CID) types for IPLD
pub mod cid;
/// Repository collection trait for records
pub mod collection;
/// Crypto helpers for keys (Multikey decoding, conversions)
pub mod crypto;
/// AT Protocol datetime string type
pub mod datetime;
/// Decentralized Identifier (DID) types and validation
pub mod did;
/// DID Document types and helpers
pub mod did_doc;
/// AT Protocol handle types and validation
pub mod handle;
/// AT Protocol identifier types (handle or DID)
pub mod ident;
/// Integer type with validation
pub mod integer;
/// Language tag types per BCP 47
pub mod language;
/// Namespaced Identifier (NSID) types and validation
pub mod nsid;
/// Record key types and validation
pub mod recordkey;
/// String types with format validation
pub mod string;
/// Timestamp Identifier (TID) types and generation
pub mod tid;
/// URI types with scheme validation
pub mod uri;
/// Generic data value types for lexicon data model
pub mod value;

/// Trait for a constant string literal type
pub trait Literal: Clone + Copy + PartialEq + Eq + Send + Sync + 'static {
    /// The string literal
    const LITERAL: &'static str;
}

/// top-level domains which are not allowed in at:// handles or dids
pub const DISALLOWED_TLDS: &[&str] = &[
    ".local",
    ".arpa",
    ".invalid", // NOTE: if someone has a screwed up handle, this is what's returned
    ".localhost",
    ".internal",
    ".example",
    ".alt",
    // policy could concievably change on ".onion" some day
    ".onion",
    // NOTE: .test is allowed in testing and devopment. In practical terms
    // "should" "never" actually resolve and get registered in production
];

/// checks if a string ends with anything from the provided list of strings.
pub fn ends_with(string: impl AsRef<str>, list: &[&str]) -> bool {
    let string = string.as_ref();
    for item in list {
        if string.ends_with(item) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "kebab-case")]
/// Valid types in the AT protocol [data model](https://atproto.com/specs/data-model). Type marker only, used in concert with `[Data<'_>]`.
pub enum DataModelType {
    /// Null type. IPLD type `null`, JSON type `Null`, CBOR Special Value (major 7)
    Null,
    /// Boolean type. IPLD type `boolean`, JSON type Boolean, CBOR Special Value (major 7)
    Boolean,
    /// Integer type. IPLD type `integer`, JSON type Number, CBOR Special Value (major 7)
    Integer,
    /// Byte type. IPLD type `bytes`, in JSON a `{ "$bytes": bytes }` Object, CBOR Byte String (major 2)
    Bytes,
    /// CID (content identifier) link. IPLD type `link`, in JSON a `{ "$link": cid }` Object, CBOR CID (tag 42)
    CidLink,
    /// Blob type. No special IPLD type. in JSON a `{ "$type": "blob" }` Object. in CBOR a `{ "$type": "blob" }` Map.
    Blob,
    /// Array type. IPLD type `list`. JSON type `Array`, CBOR type Array (major 4)
    Array,
    /// Object type. IPLD type `map`. JSON type `Object`, CBOR type Map (major 5). keys are always SmolStr.
    Object,
    #[serde(untagged)]
    /// String type (lots of variants). JSON String, CBOR UTF-8 String (major 3)
    String(LexiconStringType),
}

/// Lexicon string format types for typed strings in the AT Protocol data model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum LexiconStringType {
    /// ISO 8601 datetime string
    Datetime,
    /// AT Protocol URI (at://)
    AtUri,
    /// Decentralized Identifier
    Did,
    /// AT Protocol handle
    Handle,
    /// Handle or DID
    AtIdentifier,
    /// Namespaced Identifier
    Nsid,
    /// Content Identifier
    Cid,
    /// BCP 47 language tag
    Language,
    /// Timestamp Identifier
    Tid,
    /// Record key
    RecordKey,
    /// URI with type constraint
    Uri(UriType),
    /// Plain string
    #[serde(untagged)]
    String,
}

/// URI scheme types for lexicon URI format constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UriType {
    /// DID URI (did:)
    Did,
    /// AT Protocol URI (at://)
    At,
    /// HTTPS URI
    Https,
    /// WebSocket Secure URI
    Wss,
    /// CID URI
    Cid,
    /// DNS name
    Dns,
    /// Any valid URI
    Any,
}
