// Forked from atrium-lexicon
// https://github.com/atrium-rs/atrium/blob/main/lexicon/atrium-lex/src/lexicon.rs
// https://github.com/atrium-rs/atrium/blob/main/lexicon/atrium-lex/src/lib.rs

use jacquard_common::{CowStr, into_static::IntoStatic, smol_str::SmolStr, types::blob::MimeType};
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
    // lexObject
    Object(LexObject<'s>),
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
    // lexObject (nested)
    Object(LexObject<'s>),
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

// IntoStatic implementations for all lexicon types
// These enable converting borrowed lexicon docs to owned 'static versions

#[allow(unused)]
macro_rules! impl_into_static_for_lex_struct {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl IntoStatic for $ty<'_> {
                type Output = $ty<'static>;

                fn into_static(self) -> Self::Output {
                    let Self {
                        $(description,)?
                        ..$fields
                    } = self;
                    Self::Output {
                        $(description: description.into_static(),)?
                        ..$fields.into_static()
                    }
                }
            }
        )+
    };
}

// Simpler approach: just clone and convert each field
impl IntoStatic for Lexicon {
    type Output = Lexicon;
    fn into_static(self) -> Self::Output {
        self
    }
}

impl IntoStatic for LexStringFormat {
    type Output = LexStringFormat;
    fn into_static(self) -> Self::Output {
        self
    }
}

impl IntoStatic for LexiconDoc<'_> {
    type Output = LexiconDoc<'static>;
    fn into_static(self) -> Self::Output {
        LexiconDoc {
            lexicon: self.lexicon,
            id: self.id.into_static(),
            revision: self.revision,
            description: self.description.into_static(),
            defs: self.defs.into_static(),
        }
    }
}

impl IntoStatic for LexBoolean<'_> {
    type Output = LexBoolean<'static>;
    fn into_static(self) -> Self::Output {
        LexBoolean {
            description: self.description.into_static(),
            default: self.default,
            r#const: self.r#const,
        }
    }
}

impl IntoStatic for LexInteger<'_> {
    type Output = LexInteger<'static>;
    fn into_static(self) -> Self::Output {
        LexInteger {
            description: self.description.into_static(),
            default: self.default,
            minimum: self.minimum,
            maximum: self.maximum,
            r#enum: self.r#enum,
            r#const: self.r#const,
        }
    }
}

impl IntoStatic for LexString<'_> {
    type Output = LexString<'static>;
    fn into_static(self) -> Self::Output {
        LexString {
            description: self.description.into_static(),
            format: self.format,
            default: self.default.into_static(),
            min_length: self.min_length,
            max_length: self.max_length,
            min_graphemes: self.min_graphemes,
            max_graphemes: self.max_graphemes,
            r#enum: self.r#enum.into_static(),
            r#const: self.r#const.into_static(),
            known_values: self.known_values.into_static(),
        }
    }
}

impl IntoStatic for LexUnknown<'_> {
    type Output = LexUnknown<'static>;
    fn into_static(self) -> Self::Output {
        LexUnknown {
            description: self.description.into_static(),
        }
    }
}

impl IntoStatic for LexBytes<'_> {
    type Output = LexBytes<'static>;
    fn into_static(self) -> Self::Output {
        LexBytes {
            description: self.description.into_static(),
            max_length: self.max_length,
            min_length: self.min_length,
        }
    }
}

impl IntoStatic for LexCidLink<'_> {
    type Output = LexCidLink<'static>;
    fn into_static(self) -> Self::Output {
        LexCidLink {
            description: self.description.into_static(),
        }
    }
}

impl IntoStatic for LexRef<'_> {
    type Output = LexRef<'static>;
    fn into_static(self) -> Self::Output {
        LexRef {
            description: self.description.into_static(),
            r#ref: self.r#ref.into_static(),
        }
    }
}

impl IntoStatic for LexRefUnion<'_> {
    type Output = LexRefUnion<'static>;
    fn into_static(self) -> Self::Output {
        LexRefUnion {
            description: self.description.into_static(),
            refs: self.refs.into_static(),
            closed: self.closed,
        }
    }
}

impl IntoStatic for LexBlob<'_> {
    type Output = LexBlob<'static>;
    fn into_static(self) -> Self::Output {
        LexBlob {
            description: self.description.into_static(),
            accept: self.accept.into_static(),
            max_size: self.max_size,
        }
    }
}

impl IntoStatic for LexArrayItem<'_> {
    type Output = LexArrayItem<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Boolean(x) => LexArrayItem::Boolean(x.into_static()),
            Self::Integer(x) => LexArrayItem::Integer(x.into_static()),
            Self::String(x) => LexArrayItem::String(x.into_static()),
            Self::Unknown(x) => LexArrayItem::Unknown(x.into_static()),
            Self::Bytes(x) => LexArrayItem::Bytes(x.into_static()),
            Self::CidLink(x) => LexArrayItem::CidLink(x.into_static()),
            Self::Blob(x) => LexArrayItem::Blob(x.into_static()),
            Self::Object(x) => LexArrayItem::Object(x.into_static()),
            Self::Ref(x) => LexArrayItem::Ref(x.into_static()),
            Self::Union(x) => LexArrayItem::Union(x.into_static()),
        }
    }
}

impl IntoStatic for LexArray<'_> {
    type Output = LexArray<'static>;
    fn into_static(self) -> Self::Output {
        LexArray {
            description: self.description.into_static(),
            items: self.items.into_static(),
            min_length: self.min_length,
            max_length: self.max_length,
        }
    }
}

impl IntoStatic for LexPrimitiveArrayItem<'_> {
    type Output = LexPrimitiveArrayItem<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Boolean(x) => LexPrimitiveArrayItem::Boolean(x.into_static()),
            Self::Integer(x) => LexPrimitiveArrayItem::Integer(x.into_static()),
            Self::String(x) => LexPrimitiveArrayItem::String(x.into_static()),
            Self::Unknown(x) => LexPrimitiveArrayItem::Unknown(x.into_static()),
        }
    }
}

impl IntoStatic for LexPrimitiveArray<'_> {
    type Output = LexPrimitiveArray<'static>;
    fn into_static(self) -> Self::Output {
        LexPrimitiveArray {
            description: self.description.into_static(),
            items: self.items.into_static(),
            min_length: self.min_length,
            max_length: self.max_length,
        }
    }
}

impl IntoStatic for LexToken<'_> {
    type Output = LexToken<'static>;
    fn into_static(self) -> Self::Output {
        LexToken {
            description: self.description.into_static(),
        }
    }
}

impl IntoStatic for LexObjectProperty<'_> {
    type Output = LexObjectProperty<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Ref(x) => LexObjectProperty::Ref(x.into_static()),
            Self::Union(x) => LexObjectProperty::Union(x.into_static()),
            Self::Bytes(x) => LexObjectProperty::Bytes(x.into_static()),
            Self::CidLink(x) => LexObjectProperty::CidLink(x.into_static()),
            Self::Array(x) => LexObjectProperty::Array(x.into_static()),
            Self::Blob(x) => LexObjectProperty::Blob(x.into_static()),
            Self::Object(x) => LexObjectProperty::Object(x.into_static()),
            Self::Boolean(x) => LexObjectProperty::Boolean(x.into_static()),
            Self::Integer(x) => LexObjectProperty::Integer(x.into_static()),
            Self::String(x) => LexObjectProperty::String(x.into_static()),
            Self::Unknown(x) => LexObjectProperty::Unknown(x.into_static()),
        }
    }
}

impl IntoStatic for LexObject<'_> {
    type Output = LexObject<'static>;
    fn into_static(self) -> Self::Output {
        LexObject {
            description: self.description.into_static(),
            required: self.required,
            nullable: self.nullable,
            properties: self.properties.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcParametersProperty<'_> {
    type Output = LexXrpcParametersProperty<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Boolean(x) => LexXrpcParametersProperty::Boolean(x.into_static()),
            Self::Integer(x) => LexXrpcParametersProperty::Integer(x.into_static()),
            Self::String(x) => LexXrpcParametersProperty::String(x.into_static()),
            Self::Unknown(x) => LexXrpcParametersProperty::Unknown(x.into_static()),
            Self::Array(x) => LexXrpcParametersProperty::Array(x.into_static()),
        }
    }
}

impl IntoStatic for LexXrpcParameters<'_> {
    type Output = LexXrpcParameters<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcParameters {
            description: self.description.into_static(),
            required: self.required,
            properties: self.properties.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcBodySchema<'_> {
    type Output = LexXrpcBodySchema<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Ref(x) => LexXrpcBodySchema::Ref(x.into_static()),
            Self::Union(x) => LexXrpcBodySchema::Union(x.into_static()),
            Self::Object(x) => LexXrpcBodySchema::Object(x.into_static()),
        }
    }
}

impl IntoStatic for LexXrpcBody<'_> {
    type Output = LexXrpcBody<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcBody {
            description: self.description.into_static(),
            encoding: self.encoding.into_static(),
            schema: self.schema.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcSubscriptionMessageSchema<'_> {
    type Output = LexXrpcSubscriptionMessageSchema<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Ref(x) => LexXrpcSubscriptionMessageSchema::Ref(x.into_static()),
            Self::Union(x) => LexXrpcSubscriptionMessageSchema::Union(x.into_static()),
            Self::Object(x) => LexXrpcSubscriptionMessageSchema::Object(x.into_static()),
        }
    }
}

impl IntoStatic for LexXrpcSubscriptionMessage<'_> {
    type Output = LexXrpcSubscriptionMessage<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcSubscriptionMessage {
            description: self.description.into_static(),
            schema: self.schema.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcError<'_> {
    type Output = LexXrpcError<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcError {
            description: self.description.into_static(),
            name: self.name.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcQueryParameter<'_> {
    type Output = LexXrpcQueryParameter<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Params(x) => LexXrpcQueryParameter::Params(x.into_static()),
        }
    }
}

impl IntoStatic for LexXrpcQuery<'_> {
    type Output = LexXrpcQuery<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcQuery {
            description: self.description.into_static(),
            parameters: self.parameters.into_static(),
            output: self.output.into_static(),
            errors: self.errors.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcProcedureParameter<'_> {
    type Output = LexXrpcProcedureParameter<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Params(x) => LexXrpcProcedureParameter::Params(x.into_static()),
        }
    }
}

impl IntoStatic for LexXrpcProcedure<'_> {
    type Output = LexXrpcProcedure<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcProcedure {
            description: self.description.into_static(),
            parameters: self.parameters.into_static(),
            input: self.input.into_static(),
            output: self.output.into_static(),
            errors: self.errors.into_static(),
        }
    }
}

impl IntoStatic for LexXrpcSubscriptionParameter<'_> {
    type Output = LexXrpcSubscriptionParameter<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Params(x) => LexXrpcSubscriptionParameter::Params(x.into_static()),
        }
    }
}

impl IntoStatic for LexXrpcSubscription<'_> {
    type Output = LexXrpcSubscription<'static>;
    fn into_static(self) -> Self::Output {
        LexXrpcSubscription {
            description: self.description.into_static(),
            parameters: self.parameters.into_static(),
            message: self.message.into_static(),
            infos: self.infos.into_static(),
            errors: self.errors.into_static(),
        }
    }
}

impl IntoStatic for LexRecordRecord<'_> {
    type Output = LexRecordRecord<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Object(x) => LexRecordRecord::Object(x.into_static()),
        }
    }
}

impl IntoStatic for LexRecord<'_> {
    type Output = LexRecord<'static>;
    fn into_static(self) -> Self::Output {
        LexRecord {
            description: self.description.into_static(),
            key: self.key.into_static(),
            record: self.record.into_static(),
        }
    }
}

impl IntoStatic for LexUserType<'_> {
    type Output = LexUserType<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            Self::Record(x) => LexUserType::Record(x.into_static()),
            Self::XrpcQuery(x) => LexUserType::XrpcQuery(x.into_static()),
            Self::XrpcProcedure(x) => LexUserType::XrpcProcedure(x.into_static()),
            Self::XrpcSubscription(x) => LexUserType::XrpcSubscription(x.into_static()),
            Self::Blob(x) => LexUserType::Blob(x.into_static()),
            Self::Array(x) => LexUserType::Array(x.into_static()),
            Self::Token(x) => LexUserType::Token(x.into_static()),
            Self::Object(x) => LexUserType::Object(x.into_static()),
            Self::Boolean(x) => LexUserType::Boolean(x.into_static()),
            Self::Integer(x) => LexUserType::Integer(x.into_static()),
            Self::String(x) => LexUserType::String(x.into_static()),
            Self::Bytes(x) => LexUserType::Bytes(x.into_static()),
            Self::CidLink(x) => LexUserType::CidLink(x.into_static()),
            Self::Unknown(x) => LexUserType::Unknown(x.into_static()),
        }
    }
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
