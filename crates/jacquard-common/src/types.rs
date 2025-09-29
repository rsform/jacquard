use serde::{Deserialize, Serialize};

pub mod aturi;
pub mod blob;
pub mod cid;
pub mod collection;
pub mod datetime;
pub mod did;
pub mod handle;
pub mod ident;
pub mod integer;
pub mod language;
pub mod link;
pub mod nsid;
pub mod recordkey;
pub mod string;
pub mod tid;
pub mod uri;
pub mod value;

/// Trait for a constant string literal type
pub trait Literal: Clone + Copy + PartialEq + Eq + Send + Sync + 'static {
    /// The string literal
    const LITERAL: &'static str;
}

pub const DISALLOWED_TLDS: &[&str] = &[
    ".local",
    ".arpa",
    ".invalid",
    ".localhost",
    ".internal",
    ".example",
    ".alt",
    // policy could concievably change on ".onion" some day
    ".onion",
    // NOTE: .test is allowed in testing and devopment. In practical terms
    // "should" "never" actually resolve and get registered in production
];

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
pub enum DataModelType {
    Null,
    Boolean,
    Integer,
    Bytes,
    CidLink,
    Blob,
    Array,
    Object,
    #[serde(untagged)]
    String(LexiconStringType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum LexiconType {
    Params,
    Token,
    Ref,
    Union,
    Unknown,
    Record,
    Query,
    Procedure,
    Subscription,
    #[serde(untagged)]
    DataModel(DataModelType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum LexiconStringType {
    Datetime,
    AtUri,
    Did,
    Handle,
    AtIdentifier,
    Nsid,
    Cid,
    Language,
    Tid,
    RecordKey,
    Uri(UriType),
    #[serde(untagged)]
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UriType {
    Did,
    At,
    Https,
    Wss,
    Cid,
    Dns,
    Any,
}
