// Forked from atrium-lexicon
// https://github.com/atrium-rs/atrium/blob/main/lexicon/atrium-lex/src/lexicon.rs
// https://github.com/atrium-rs/atrium/blob/main/lexicon/atrium-lex/src/lib.rs

use jacquard_common::{CowStr, smol_str::SmolStr, types::blob::MimeType};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::skip_serializing_none;
use std::collections::BTreeMap;

#[derive(Debug, Serialize_repr, Deserialize_repr, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum Lexicon {
    Lexicon1 = 1,
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexiconDoc<'s> {
    pub lexicon: Lexicon,
    #[serde(borrow)]
    pub id: CowStr<'s>,
    pub revision: Option<u32>,
    pub description: Option<CowStr<'s>>,
    pub defs: BTreeMap<SmolStr, LexUserType<'s>>,
}

// primitives

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexBoolean<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub default: Option<bool>,
    pub r#const: Option<bool>,
}

/// The Lexicon type `integer`.
///
/// Lexicon integers are [specified] as signed and 64-bit, which means that values will
/// always fit in an `i64`.
///
/// [specified]: https://atproto.com/specs/data-model#data-types
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexInteger<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub default: Option<i64>,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub r#enum: Option<Vec<i64>>,
    pub r#const: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum LexStringFormat {
    Datetime,
    Uri,
    AtUri,
    Did,
    Handle,
    AtIdentifier,
    Nsid,
    Cid,
    Language,
    Tid,
    RecordKey,
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LexString<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub format: Option<LexStringFormat>,
    pub default: Option<CowStr<'s>>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_graphemes: Option<usize>,
    pub max_graphemes: Option<usize>,
    pub r#enum: Option<Vec<CowStr<'s>>>,
    pub r#const: Option<CowStr<'s>>,
    pub known_values: Option<Vec<CowStr<'s>>>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexUnknown<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
}
// ipld types

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LexBytes<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub max_length: Option<usize>,
    pub min_length: Option<usize>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexCidLink<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
}

// references

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexRef<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub r#ref: CowStr<'s>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexRefUnion<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub refs: Vec<CowStr<'s>>,
    pub closed: Option<bool>,
}

// blobs

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LexBlob<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub accept: Option<Vec<MimeType<'s>>>,
    pub max_size: Option<usize>,
}

// complex types

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LexArrayItem<'s> {
    // lexPrimitive
    Boolean(LexBoolean<'s>),
    Integer(LexInteger<'s>),
    String(LexString<'s>),
    Unknown(LexUnknown<'s>),
    // lexIpldType
    Bytes(LexBytes<'s>),
    CidLink(LexCidLink<'s>),
    // lexBlob
    #[serde(borrow)]
    Blob(LexBlob<'s>),
    // lexRefVariant
    Ref(LexRef<'s>),
    Union(LexRefUnion<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LexArray<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub items: LexArrayItem<'s>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexPrimitiveArrayItem<'s> {
    // lexPrimitive
    #[serde(borrow)]
    Boolean(LexBoolean<'s>),
    Integer(LexInteger<'s>),
    String(LexString<'s>),
    Unknown(LexUnknown<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LexPrimitiveArray<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub items: LexPrimitiveArrayItem<'s>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexToken<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LexObjectProperty<'s> {
    // lexRefVariant
    #[serde(borrow)]
    Ref(LexRef<'s>),
    Union(LexRefUnion<'s>),
    // lexIpldType
    Bytes(LexBytes<'s>),
    CidLink(LexCidLink<'s>),
    // lexArray
    Array(LexArray<'s>),
    // lexBlob
    Blob(LexBlob<'s>),
    // lexPrimitive
    Boolean(LexBoolean<'s>),
    Integer(LexInteger<'s>),
    String(LexString<'s>),
    Unknown(LexUnknown<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexObject<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub required: Option<Vec<SmolStr>>,
    pub nullable: Option<Vec<SmolStr>>,
    pub properties: BTreeMap<SmolStr, LexObjectProperty<'s>>,
}

// xrpc

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexXrpcParametersProperty<'s> {
    // lexPrimitive
    #[serde(borrow)]
    Boolean(LexBoolean<'s>),
    Integer(LexInteger<'s>),
    String(LexString<'s>),
    Unknown(LexUnknown<'s>),
    // lexPrimitiveArray
    Array(LexPrimitiveArray<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcParameters<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub required: Option<Vec<SmolStr>>,
    pub properties: BTreeMap<SmolStr, LexXrpcParametersProperty<'s>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexXrpcBodySchema<'s> {
    // lexRefVariant
    #[serde(borrow)]
    Ref(LexRef<'s>),
    Union(LexRefUnion<'s>),
    // lexObject
    Object(LexObject<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcBody<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub encoding: CowStr<'s>,
    pub schema: Option<LexXrpcBodySchema<'s>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexXrpcSubscriptionMessageSchema<'s> {
    // lexRefVariant
    #[serde(borrow)]
    Ref(LexRef<'s>),
    Union(LexRefUnion<'s>),
    // lexObject
    Object(LexObject<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcSubscriptionMessage<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub schema: Option<LexXrpcSubscriptionMessageSchema<'s>>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcError<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub name: CowStr<'s>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexXrpcQueryParameter<'s> {
    #[serde(borrow)]
    Params(LexXrpcParameters<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcQuery<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub parameters: Option<LexXrpcQueryParameter<'s>>,
    pub output: Option<LexXrpcBody<'s>>,
    pub errors: Option<Vec<LexXrpcError<'s>>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexXrpcProcedureParameter<'s> {
    #[serde(borrow)]
    Params(LexXrpcParameters<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcProcedure<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub parameters: Option<LexXrpcProcedureParameter<'s>>,
    pub input: Option<LexXrpcBody<'s>>,
    pub output: Option<LexXrpcBody<'s>>,
    pub errors: Option<Vec<LexXrpcError<'s>>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexXrpcSubscriptionParameter<'s> {
    #[serde(borrow)]
    Params(LexXrpcParameters<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexXrpcSubscription<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub parameters: Option<LexXrpcSubscriptionParameter<'s>>,
    pub message: Option<LexXrpcSubscriptionMessage<'s>>,
    pub infos: Option<Vec<LexXrpcError<'s>>>,
    pub errors: Option<Vec<LexXrpcError<'s>>>,
}

// database

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LexRecordRecord<'s> {
    #[serde(borrow)]
    Object(LexObject<'s>),
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexRecord<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub key: Option<CowStr<'s>>,
    pub record: LexRecordRecord<'s>,
}

// core

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LexUserType<'s> {
    // lexRecord
    #[serde(borrow)]
    Record(LexRecord<'s>),
    // lexXrpcQuery
    #[serde(rename = "query")]
    XrpcQuery(LexXrpcQuery<'s>),
    // lexXrpcProcedure
    #[serde(rename = "procedure")]
    XrpcProcedure(LexXrpcProcedure<'s>),
    // lexXrpcSubscription
    #[serde(rename = "subscription")]
    XrpcSubscription(LexXrpcSubscription<'s>),
    // lexBlob
    Blob(LexBlob<'s>),
    // lexArray
    Array(LexArray<'s>),
    // lexToken
    Token(LexToken<'s>),
    // lexObject
    Object(LexObject<'s>),
    // lexBoolean,
    Boolean(LexBoolean<'s>),
    // lexInteger,
    Integer(LexInteger<'s>),
    // lexString,
    String(LexString<'s>),
    // lexBytes
    Bytes(LexBytes<'s>),
    // lexCidLink
    CidLink(LexCidLink<'s>),
    // lexUnknown
    Unknown(LexUnknown<'s>),
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEXICON_EXAMPLE_TOKEN: &str = r#"
{
  "lexicon": 1,
  "id": "com.socialapp.actorUser",
  "defs": {
    "main": {
      "type": "token",
      "description": "Actor type of 'User'"
    }
  }
}"#;

    #[test]
    fn parse() {
        let doc = serde_json::from_str::<LexiconDoc>(LEXICON_EXAMPLE_TOKEN)
            .expect("failed to deserialize");
        assert_eq!(doc.lexicon, Lexicon::Lexicon1);
        assert_eq!(doc.id, "com.socialapp.actorUser");
        assert_eq!(doc.revision, None);
        assert_eq!(doc.description, None);
        assert_eq!(doc.defs.len(), 1);
    }
}
