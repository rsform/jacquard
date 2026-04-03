// Forked from atrium-lexicon
// https://github.com/atrium-rs/atrium/blob/main/lexicon/atrium-lex/src/lexicon.rs
// https://github.com/atrium-rs/atrium/blob/main/lexicon/atrium-lex/src/lib.rs

use jacquard_common::{
    CowStr, deps::smol_str::SmolStr, into_static::IntoStatic, types::blob::MimeType,
    types::did::Did, types::nsid::Nsid,
    types::scope_primitives::{RepoAction, AccountAction},
};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::skip_serializing_none;
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

#[derive(Debug, Serialize_repr, Deserialize_repr, PartialEq, Eq, Clone, Copy, Default)]
#[repr(u8)]
pub enum Lexicon {
    #[default]
    Lexicon1 = 1,
}
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct LexUnknown<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
}
// ipld types

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LexBytes<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub max_length: Option<usize>,
    pub min_length: Option<usize>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct LexCidLink<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
}

// references

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct LexRef<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub r#ref: CowStr<'s>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct LexRefUnion<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub refs: Vec<CowStr<'s>>,
    pub closed: Option<bool>,
}

// blobs

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LexBlob<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub accept: Option<Vec<MimeType<CowStr<'s>>>>,
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

impl<'s> Default for LexArrayItem<'s> {
    fn default() -> Self {
        LexArrayItem::Unknown(LexUnknown::default())
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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

impl<'s> Default for LexPrimitiveArrayItem<'s> {
    fn default() -> Self {
        LexPrimitiveArrayItem::Unknown(LexUnknown::default())
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LexPrimitiveArray<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub items: LexPrimitiveArrayItem<'s>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct LexXrpcSubscriptionMessage<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub schema: Option<LexXrpcSubscriptionMessageSchema<'s>>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
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

impl<'s> Default for LexRecordRecord<'s> {
    fn default() -> Self {
        Self::Object(LexObject::default())
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct LexRecord<'s> {
    #[serde(borrow)]
    pub description: Option<CowStr<'s>>,
    pub key: Option<CowStr<'s>>,
    pub record: LexRecordRecord<'s>,
}

// permission sets

/// AT Protocol permission set lexicon type.
///
/// Contains a `permissions` array where each entry is a `LexPermission`
/// with `"type": "permission"` and a `"resource"` discriminator carrying
/// typed fields (NSIDs, DIDs, MIME types, action enums).
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct LexPermissionSet<'s> {
    #[serde(borrow)]
    pub title: Option<CowStr<'s>>,
    #[serde(default, rename = "title:lang")]
    pub title_lang: Option<HashMap<CowStr<'s>, CowStr<'s>>>,
    pub detail: Option<CowStr<'s>>,
    #[serde(default, rename = "detail:lang")]
    pub detail_lang: Option<HashMap<CowStr<'s>, CowStr<'s>>>,
    pub permissions: Vec<LexPermission<'s>>,
}

/// A permission entry within a permission set.
///
/// Single-variant enum: the `"type": "permission"` JSON tag selects the
/// `Permission` variant, which wraps a `LexPermissionResource` discriminated
/// by the `"resource"` field.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LexPermission<'s> {
    /// A permission entry.
    Permission {
        #[serde(flatten, borrow)]
        resource: LexPermissionResource<'s>,
    },
}

/// Resource-specific permission data, discriminated by the `"resource"` field.
#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(tag = "resource", rename_all = "kebab-case")]
pub enum LexPermissionResource<'s> {
    /// Repository resource permission.
    Repo {
        /// Collection NSIDs this permission applies to.
        #[serde(borrow)]
        collection: Vec<Nsid<CowStr<'s>>>,
        /// Permitted actions (create, update, delete). None = all actions.
        #[serde(default)]
        action: Option<Vec<RepoAction>>,
    },
    /// RPC method permission.
    Rpc {
        /// Lexicon method NSIDs this permission applies to.
        #[serde(borrow)]
        lxm: Vec<Nsid<CowStr<'s>>>,
        /// Audience DID for inter-service auth.
        #[serde(borrow, default)]
        aud: Option<Did<CowStr<'s>>>,
        /// If true, inherits audience from the include scope's aud parameter.
        #[serde(default, rename = "inheritAud")]
        inherit_aud: Option<bool>,
    },
    /// Blob resource permission.
    Blob {
        /// Accepted MIME type patterns.
        #[serde(borrow)]
        accept: Vec<MimeType<CowStr<'s>>>,
        /// Maximum blob size in bytes.
        #[serde(default)]
        max_size: Option<u64>,
    },
    /// Identity resource permission.
    Identity {
        /// Identity attribute (e.g., "handle").
        #[serde(borrow)]
        attr: CowStr<'s>,
    },
    /// Account resource permission.
    Account {
        /// Account attribute (e.g., "email").
        #[serde(borrow)]
        attr: CowStr<'s>,
        /// Permitted actions (read, manage). None = read.
        #[serde(default)]
        action: Option<Vec<AccountAction>>,
    },
}

/// Errors from permission set validation.
#[derive(Debug, Error)]
pub enum PermissionSetError {
    #[error("permission set has empty permissions array")]
    EmptyPermissions,

    #[error("permission set {nsid} references out-of-namespace resource: {resource}")]
    NamespaceViolation { nsid: String, resource: String },
}

impl<'s> LexPermissionSet<'s> {
    /// Validate the permission set against its owning NSID.
    ///
    /// Checks:
    /// 1. Permissions array is non-empty
    /// 2. All NSID-scoped resources (collection, lxm) are within the
    ///    owning NSID's namespace (first two segments)
    pub fn validate(&self, owning_nsid: &str) -> Result<(), PermissionSetError> {
        if self.permissions.is_empty() {
            return Err(PermissionSetError::EmptyPermissions);
        }

        let namespace = {
            let mut parts = owning_nsid.splitn(3, '.');
            match (parts.next(), parts.next()) {
                (Some(a), Some(b)) => format!("{}.{}", a, b),
                _ => owning_nsid.to_string(),
            }
        };

        for perm in &self.permissions {
            let LexPermission::Permission { resource } = perm;
            match resource {
                LexPermissionResource::Repo { collection, .. } => {
                    for col in collection {
                        let col_str: &str = col.as_ref();
                        if !col_str.starts_with(&format!("{}.", namespace)) {
                            return Err(PermissionSetError::NamespaceViolation {
                                nsid: owning_nsid.to_string(),
                                resource: col_str.to_string(),
                            });
                        }
                    }
                }
                LexPermissionResource::Rpc { lxm, .. } => {
                    for l in lxm {
                        let lxm_str: &str = l.as_ref();
                        if !lxm_str.starts_with(&format!("{}.", namespace)) {
                            return Err(PermissionSetError::NamespaceViolation {
                                nsid: owning_nsid.to_string(),
                                resource: lxm_str.to_string(),
                            });
                        }
                    }
                }
                // Blob, Identity, Account don't have namespace-scoped NSID resources.
                _ => {}
            }
        }

        Ok(())
    }
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
    // lexRefUnion
    Union(LexRefUnion<'s>),
    // lexPermissionSet
    #[serde(borrow)]
    PermissionSet(LexPermissionSet<'s>),
}

// IntoStatic implementations for all lexicon types
// These enable converting borrowed lexicon docs to owned 'static versions

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

impl IntoStatic for LexPermissionResource<'_> {
    type Output = LexPermissionResource<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            LexPermissionResource::Repo { collection, action } => LexPermissionResource::Repo {
                collection: collection.into_iter().map(|n| n.into_static()).collect(),
                action,
            },
            LexPermissionResource::Rpc {
                lxm,
                aud,
                inherit_aud,
            } => LexPermissionResource::Rpc {
                lxm: lxm.into_iter().map(|n| n.into_static()).collect(),
                aud: aud.map(|d| d.into_static()),
                inherit_aud,
            },
            LexPermissionResource::Blob { accept, max_size } => LexPermissionResource::Blob {
                accept: accept.into_iter().map(|a| a.into_static()).collect(),
                max_size,
            },
            LexPermissionResource::Identity { attr } => LexPermissionResource::Identity {
                attr: attr.into_static(),
            },
            LexPermissionResource::Account { attr, action } => LexPermissionResource::Account {
                attr: attr.into_static(),
                action,
            },
        }
    }
}

impl IntoStatic for LexPermission<'_> {
    type Output = LexPermission<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            LexPermission::Permission { resource } => LexPermission::Permission {
                resource: resource.into_static(),
            },
        }
    }
}

impl IntoStatic for LexPermissionSet<'_> {
    type Output = LexPermissionSet<'static>;

    fn into_static(self) -> Self::Output {
        LexPermissionSet {
            title: self.title.into_static(),
            title_lang: self.title_lang.into_static(),
            detail: self.detail.into_static(),
            detail_lang: self.detail_lang.into_static(),
            permissions: self
                .permissions
                .into_iter()
                .map(|p| p.into_static())
                .collect(),
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
            Self::Union(x) => LexUserType::Union(x.into_static()),
            Self::PermissionSet(x) => LexUserType::PermissionSet(x.into_static()),
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

    // Permission set tests for oauth-scopes-rework.AC5

    const PERMISSION_SET_SIMPLE: &str = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authFull",
  "defs": {
    "main": {
      "type": "permission-set",
      "title": "Full Bluesky Client Access",
      "detail": "Allows reading and writing to Bluesky records",
      "permissions": [
        {
          "type": "permission",
          "resource": "repo",
          "collection": ["app.bsky.feed.post"],
          "action": ["create"]
        }
      ]
    }
  }
}
"#;

    const PERMISSION_SET_FULL: &str = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authFull",
  "defs": {
    "main": {
      "type": "permission-set",
      "title": "Full Bluesky Client Access",
      "title:lang": {
        "es": "Acceso completo al cliente de Bluesky"
      },
      "detail": "Allows reading and writing to Bluesky records and making service calls",
      "detail:lang": {
        "es": "Permite leer y escribir registros de Bluesky y realizar llamadas de servicio"
      },
      "permissions": [
        {
          "type": "permission",
          "resource": "repo",
          "collection": ["app.bsky.feed.post", "app.bsky.feed.like"],
          "action": ["create", "update", "delete"]
        },
        {
          "type": "permission",
          "resource": "repo",
          "collection": ["app.bsky.actor.profile"],
          "action": ["update"]
        },
        {
          "type": "permission",
          "resource": "rpc",
          "lxm": ["app.bsky.feed.getLikes", "app.bsky.feed.getAuthorFeed"],
          "inheritAud": true
        },
        {
          "type": "permission",
          "resource": "rpc",
          "lxm": ["app.bsky.notification.listNotifications"],
          "aud": "did:web:api.bsky.app"
        },
        {
          "type": "permission",
          "resource": "identity",
          "attr": "handle"
        },
        {
          "type": "permission",
          "resource": "account",
          "attr": "email",
          "action": ["read"]
        }
      ]
    }
  }
}
"#;

    #[test]
    fn test_permission_set_deserialize_simple() {
        let doc = serde_json::from_str::<LexiconDoc>(PERMISSION_SET_SIMPLE)
            .expect("failed to deserialize");
        assert_eq!(doc.id, "app.bsky.authFull");

        let main_def = doc.defs.get("main").expect("main def exists");
        match main_def {
            LexUserType::PermissionSet(pset) => {
                assert_eq!(pset.title.as_ref().map(|s| s.as_ref()), Some("Full Bluesky Client Access"));
                assert_eq!(pset.permissions.len(), 1);

                let perm = &pset.permissions[0];
                match perm {
                    LexPermission::Permission {
                        resource: LexPermissionResource::Repo { collection, action },
                    } => {
                        assert_eq!(collection.len(), 1);
                        assert_eq!(collection[0].as_ref(), "app.bsky.feed.post");
                        assert_eq!(
                            action.as_ref().map(|a| a.len()),
                            Some(1),
                            "has action vec"
                        );
                        if let Some(actions) = action {
                            assert_eq!(actions[0], RepoAction::Create);
                        }
                    }
                    _ => panic!("expected Repo permission"),
                }
            }
            _ => panic!("expected PermissionSet"),
        }
    }

    #[test]
    fn test_permission_set_deserialize_full() {
        let doc = serde_json::from_str::<LexiconDoc>(PERMISSION_SET_FULL)
            .expect("failed to deserialize");
        let main_def = doc.defs.get("main").expect("main def");

        match main_def {
            LexUserType::PermissionSet(pset) => {
                assert_eq!(pset.title_lang.as_ref().map(|m| m.len()), Some(1));
                assert_eq!(pset.detail_lang.as_ref().map(|m| m.len()), Some(1));
                assert_eq!(pset.permissions.len(), 6, "has 6 permission entries");

                // Entry 1: Repo with 2 collections and 3 actions
                match &pset.permissions[0] {
                    LexPermission::Permission {
                        resource: LexPermissionResource::Repo { collection, action },
                    } => {
                        assert_eq!(collection.len(), 2);
                        assert_eq!(
                            action.as_ref().map(|a| a.len()),
                            Some(3),
                            "has 3 actions"
                        );
                    }
                    _ => panic!("entry 0 should be Repo"),
                }

                // Entry 3: Rpc with inherit_aud
                match &pset.permissions[2] {
                    LexPermission::Permission {
                        resource: LexPermissionResource::Rpc { lxm, inherit_aud, .. },
                    } => {
                        assert_eq!(lxm.len(), 2);
                        assert_eq!(*inherit_aud, Some(true));
                    }
                    _ => panic!("entry 2 should be Rpc with inherit_aud"),
                }

                // Entry 4: Rpc with explicit aud
                match &pset.permissions[3] {
                    LexPermission::Permission {
                        resource: LexPermissionResource::Rpc { aud, .. },
                    } => {
                        assert!(aud.is_some(), "has aud");
                    }
                    _ => panic!("entry 3 should be Rpc with aud"),
                }

                // Entry 5: Identity
                match &pset.permissions[4] {
                    LexPermission::Permission {
                        resource: LexPermissionResource::Identity { attr },
                    } => {
                        assert_eq!(attr.as_ref(), "handle");
                    }
                    _ => panic!("entry 4 should be Identity"),
                }

                // Entry 6: Account with action
                match &pset.permissions[5] {
                    LexPermission::Permission {
                        resource: LexPermissionResource::Account { attr, action },
                    } => {
                        assert_eq!(attr.as_ref(), "email");
                        assert_eq!(
                            action.as_ref().map(|a| a.len()),
                            Some(1),
                            "has 1 action"
                        );
                        if let Some(actions) = action {
                            assert_eq!(actions[0], AccountAction::Read);
                        }
                    }
                    _ => panic!("entry 5 should be Account"),
                }
            }
            _ => panic!("expected PermissionSet"),
        }
    }

    #[test]
    fn test_permission_set_into_static() {
        let doc = serde_json::from_str::<LexiconDoc>(PERMISSION_SET_FULL)
            .expect("failed to deserialize");
        let main_def = doc
            .defs
            .get("main")
            .expect("main def")
            .clone()
            .into_static();

        match main_def {
            LexUserType::PermissionSet(pset) => {
                assert_eq!(pset.permissions.len(), 6);
                // Verify all borrowed fields are converted to 'static
                assert!(pset.title.is_some());
                assert!(pset.title_lang.is_some());
            }
            _ => panic!("expected PermissionSet"),
        }
    }

    #[test]
    fn test_permission_set_namespace_violation() {
        let doc = serde_json::from_str::<LexiconDoc>(PERMISSION_SET_SIMPLE)
            .expect("failed to deserialize");
        let pset = match doc.defs.get("main").expect("main def") {
            LexUserType::PermissionSet(p) => p,
            _ => panic!("expected PermissionSet"),
        };

        // Valid: app.bsky.feed.post is in app.bsky namespace
        assert!(pset.validate("app.bsky.authFull").is_ok());

        // Invalid: com.atproto is out of namespace for app.bsky
        let invalid_json = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authFull",
  "defs": {
    "main": {
      "type": "permission-set",
      "permissions": [
        {
          "type": "permission",
          "resource": "repo",
          "collection": ["com.atproto.repo.createRecord"],
          "action": ["create"]
        }
      ]
    }
  }
}
"#;
        let doc = serde_json::from_str::<LexiconDoc>(invalid_json).expect("deserialize");
        let pset = match doc.defs.get("main").expect("main def") {
            LexUserType::PermissionSet(p) => p,
            _ => panic!("expected PermissionSet"),
        };
        let result = pset.validate("app.bsky.authFull");
        assert!(result.is_err());
        match result.unwrap_err() {
            PermissionSetError::NamespaceViolation { nsid, resource } => {
                assert_eq!(nsid, "app.bsky.authFull");
                assert_eq!(resource, "com.atproto.repo.createRecord");
            }
            _ => panic!("expected NamespaceViolation"),
        }
    }

    #[test]
    fn test_permission_set_empty_permissions() {
        let json = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authEmpty",
  "defs": {
    "main": {
      "type": "permission-set",
      "permissions": []
    }
  }
}
"#;
        let doc = serde_json::from_str::<LexiconDoc>(json).expect("deserialize");
        let pset = match doc.defs.get("main").expect("main def") {
            LexUserType::PermissionSet(p) => p,
            _ => panic!("expected PermissionSet"),
        };
        let result = pset.validate("app.bsky.authEmpty");
        assert!(result.is_err());
        match result.unwrap_err() {
            PermissionSetError::EmptyPermissions => {}
            _ => panic!("expected EmptyPermissions"),
        }
    }

    #[test]
    fn test_permission_set_serialize_roundtrip() {
        let doc = serde_json::from_str::<LexiconDoc>(PERMISSION_SET_SIMPLE)
            .expect("failed to deserialize");
        let orig_pset = match doc.defs.get("main").expect("main") {
            LexUserType::PermissionSet(p) => p,
            _ => panic!("expected PermissionSet"),
        };

        // Serialize to JSON value and back
        let serialized_str = serde_json::to_string(orig_pset).expect("serialize to string");
        let deserialized_pset =
            serde_json::from_str::<LexPermissionSet>(serialized_str.as_str())
                .expect("roundtrip deserialize");

        assert_eq!(orig_pset.permissions.len(), deserialized_pset.permissions.len());
    }

    #[test]
    fn test_permission_set_invalid_nsid() {
        let json = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authBad",
  "defs": {
    "main": {
      "type": "permission-set",
      "permissions": [
        {
          "type": "permission",
          "resource": "repo",
          "collection": ["invalid..nsid"],
          "action": ["create"]
        }
      ]
    }
  }
}
"#;
        // Invalid NSID should fail during deserialization
        let result = serde_json::from_str::<LexiconDoc>(json);
        assert!(result.is_err(), "invalid NSID should fail deserialization");
    }

    #[test]
    fn test_permission_set_invalid_did() {
        let json = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authBad",
  "defs": {
    "main": {
      "type": "permission-set",
      "permissions": [
        {
          "type": "permission",
          "resource": "rpc",
          "lxm": ["app.bsky.feed.getLikes"],
          "aud": "not-a-did"
        }
      ]
    }
  }
}
"#;
        // Invalid DID should fail during deserialization
        let result = serde_json::from_str::<LexiconDoc>(json);
        assert!(result.is_err(), "invalid DID should fail deserialization");
    }

    #[test]
    fn test_permission_set_title_lang() {
        let doc = serde_json::from_str::<LexiconDoc>(PERMISSION_SET_FULL)
            .expect("failed to deserialize");
        let pset = match doc.defs.get("main").expect("main def") {
            LexUserType::PermissionSet(p) => p,
            _ => panic!("expected PermissionSet"),
        };

        let title_lang = pset.title_lang.as_ref().expect("has title:lang");
        assert_eq!(title_lang.len(), 1);
        let es_title = title_lang
            .iter()
            .find(|(k, _)| k.as_ref() == "es")
            .expect("has es translation");
        assert_eq!(
            es_title.1.as_ref(),
            "Acceso completo al cliente de Bluesky"
        );

        // Roundtrip and verify title:lang survives
        let serialized = serde_json::to_value(&pset).expect("serialize");
        assert!(
            serialized.get("title:lang").is_some(),
            "title:lang field preserved in JSON"
        );
    }

    #[test]
    fn test_permission_set_rpc_namespace_violation() {
        let json = r#"
{
  "lexicon": 1,
  "id": "app.bsky.authBad",
  "defs": {
    "main": {
      "type": "permission-set",
      "permissions": [
        {
          "type": "permission",
          "resource": "rpc",
          "lxm": ["com.atproto.server.createAccount"],
          "inheritAud": true
        }
      ]
    }
  }
}
"#;
        let doc = serde_json::from_str::<LexiconDoc>(json).expect("deserialize");
        let pset = match doc.defs.get("main").expect("main def") {
            LexUserType::PermissionSet(p) => p,
            _ => panic!("expected PermissionSet"),
        };
        let result = pset.validate("app.bsky.authBad");
        assert!(result.is_err(), "rpc lxm out of namespace should fail");
        match result.unwrap_err() {
            PermissionSetError::NamespaceViolation { resource, .. } => {
                assert_eq!(resource, "com.atproto.server.createAccount");
            }
            _ => panic!("expected NamespaceViolation"),
        }
    }
}
