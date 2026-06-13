//! Hand-written XRPC endpoint types for com.atproto endpoints.
//!
//! These types are vendored in jacquard-common to break the circular dependency
//! between jacquard-lexgen/jacquard-identity and jacquard-api. They provide minimal
//! implementations sufficient for bootstrap code generation without builders or
//! validation helpers.
//!
use crate::BosStr;
use crate::DefaultStr;
use crate::IntoStatic;
use crate::deps::bytes::Bytes;
use crate::error::DecodeError;
use crate::types::blob::BlobRef;
use crate::types::did_doc::DidDocument;
use crate::types::ident::AtIdentifier;
use crate::types::string::{AtUri, Cid, Did, Handle, Nsid, RecordKey, Rkey, Tid};
use crate::types::value::Data;
use crate::xrpc::{EncodeError, GenericError, XrpcMethod, XrpcRequest, XrpcResp};

use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// ============================================================================
// com.atproto.repo.listRecords
// ============================================================================

/// Request for com.atproto.repo.listRecords.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ListRecords<S = DefaultStr>
where
    S: BosStr,
{
    pub collection: Nsid<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    pub repo: AtIdentifier<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
}

impl<S> IntoStatic for ListRecords<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ListRecords<S::Output>;

    fn into_static(self) -> Self::Output {
        ListRecords {
            collection: self.collection.into_static(),
            cursor: self.cursor.into_static(),
            limit: self.limit,
            repo: self.repo.into_static(),
            reverse: self.reverse,
        }
    }
}

/// Output for com.atproto.repo.listRecords.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ListRecordsOutput<S = DefaultStr>
where
    S: BosStr,
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<S>,
    pub records: Vec<ListRecordsRecord<S>>,
}

impl<S> IntoStatic for ListRecordsOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ListRecordsOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        ListRecordsOutput {
            cursor: self.cursor.into_static(),
            records: self.records.into_static(),
        }
    }
}

/// A single record in a list response.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ListRecordsRecord<S = DefaultStr>
where
    S: BosStr,
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<Cid<S>>,
    pub uri: AtUri<S>,
    pub value: Data<S>,
}

impl<S> IntoStatic for ListRecordsRecord<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ListRecordsRecord<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ListRecordsRecord {
            cid: self.cid.into_static(),
            uri: self.uri.into_static(),
            value: self.value.into_static(),
        }
    }
}

/// Response marker for com.atproto.repo.listRecords.
pub struct ListRecordsResponse;

impl XrpcResp for ListRecordsResponse {
    const NSID: &'static str = "com.atproto.repo.listRecords";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = ListRecordsOutput<S>;
    type Err = GenericError;
}

impl<S> XrpcRequest for ListRecords<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.repo.listRecords";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ListRecordsResponse;
}

// ============================================================================
// com.atproto.repo.getRecord
// ============================================================================

/// Request for com.atproto.repo.getRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct GetRecord<S: BosStr = DefaultStr> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<Cid<S>>,
    pub collection: Nsid<S>,
    pub repo: AtIdentifier<S>,
    pub rkey: RecordKey<Rkey<S>>,
}

impl<S> IntoStatic for GetRecord<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = GetRecord<S::Output>;

    fn into_static(self) -> Self::Output {
        GetRecord {
            cid: self.cid.into_static(),
            collection: self.collection.into_static(),
            repo: self.repo.into_static(),
            rkey: self.rkey.into_static(),
        }
    }
}

/// Output for com.atproto.repo.getRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordOutput<S = DefaultStr>
where
    S: BosStr,
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<Cid<S>>,
    pub uri: AtUri<S>,
    pub value: Data<S>,
}

impl<S> IntoStatic for GetRecordOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = GetRecordOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        GetRecordOutput {
            cid: self.cid.into_static(),
            uri: self.uri.into_static(),
            value: self.value.into_static(),
        }
    }
}

/// Error type for com.atproto.repo.getRecord. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(tag = "error", content = "message")]
pub enum GetRecordError {
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(other)]
    Other,
}

impl Display for GetRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordNotFound(msg) => {
                write!(f, "RecordNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other => write!(f, "Unknown error"),
        }
    }
}

impl Error for GetRecordError {}

/// Response marker for com.atproto.repo.getRecord.
pub struct GetRecordResponse;

impl XrpcResp for GetRecordResponse {
    const NSID: &'static str = "com.atproto.repo.getRecord";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = GetRecordOutput<S>;
    type Err = GetRecordError;
}

impl<S> XrpcRequest for GetRecord<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.repo.getRecord";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = GetRecordResponse;
}

// ============================================================================
// com.atproto.repo.createRecord
// ============================================================================

/// Commit metadata returned by record write operations.
///
/// Mirrors the generated `com.atproto.repo.defs#commitMeta` type. The `rev`
/// field is a non-generic `Tid` (always `SmolStr`-backed).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct CommitMeta<S: BosStr = DefaultStr> {
    pub cid: Cid<S>,
    pub rev: Tid,
}

impl<S> IntoStatic for CommitMeta<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = CommitMeta<S::Output>;

    fn into_static(self) -> Self::Output {
        CommitMeta {
            cid: self.cid.into_static(),
            rev: self.rev,
        }
    }
}

/// Request for com.atproto.repo.createRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecord<S: BosStr = DefaultStr> {
    pub collection: Nsid<S>,
    pub record: Data<S>,
    pub repo: AtIdentifier<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rkey: Option<RecordKey<Rkey<S>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_commit: Option<Cid<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<bool>,
}

impl<S> IntoStatic for CreateRecord<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = CreateRecord<S::Output>;

    fn into_static(self) -> Self::Output {
        CreateRecord {
            collection: self.collection.into_static(),
            record: self.record.into_static(),
            repo: self.repo.into_static(),
            rkey: self.rkey.into_static(),
            swap_commit: self.swap_commit.into_static(),
            validate: self.validate,
        }
    }
}

/// Output for com.atproto.repo.createRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecordOutput<S: BosStr = DefaultStr> {
    pub cid: Cid<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitMeta<S>>,
    pub uri: AtUri<S>,
}

impl<S> IntoStatic for CreateRecordOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = CreateRecordOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        CreateRecordOutput {
            cid: self.cid.into_static(),
            commit: self.commit.into_static(),
            uri: self.uri.into_static(),
        }
    }
}

/// Error type for com.atproto.repo.createRecord. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(tag = "error", content = "message")]
pub enum CreateRecordError {
    /// Indicates that 'swapCommit' didn't match current repo commit.
    #[serde(rename = "InvalidSwap")]
    InvalidSwap(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(untagged)]
    Other {
        error: SmolStr,
        message: Option<SmolStr>,
    },
}

impl Display for CreateRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSwap(msg) => {
                write!(f, "InvalidSwap")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other { error, message } => {
                write!(f, "{}", error)?;
                if let Some(msg) = message {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for CreateRecordError {}

/// Response marker for com.atproto.repo.createRecord.
pub struct CreateRecordResponse;

impl XrpcResp for CreateRecordResponse {
    const NSID: &'static str = "com.atproto.repo.createRecord";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = CreateRecordOutput<S>;
    type Err = CreateRecordError;
}

impl<S> XrpcRequest for CreateRecord<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.repo.createRecord";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Response = CreateRecordResponse;
}

// ============================================================================
// com.atproto.repo.putRecord
// ============================================================================

/// Request for com.atproto.repo.putRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct PutRecord<S: BosStr = DefaultStr> {
    pub collection: Nsid<S>,
    pub record: Data<S>,
    pub repo: AtIdentifier<S>,
    pub rkey: RecordKey<Rkey<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_commit: Option<Cid<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_record: Option<Cid<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<bool>,
}

impl<S> IntoStatic for PutRecord<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = PutRecord<S::Output>;

    fn into_static(self) -> Self::Output {
        PutRecord {
            collection: self.collection.into_static(),
            record: self.record.into_static(),
            repo: self.repo.into_static(),
            rkey: self.rkey.into_static(),
            swap_commit: self.swap_commit.into_static(),
            swap_record: self.swap_record.into_static(),
            validate: self.validate,
        }
    }
}

/// Output for com.atproto.repo.putRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct PutRecordOutput<S: BosStr = DefaultStr> {
    pub cid: Cid<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitMeta<S>>,
    pub uri: AtUri<S>,
}

impl<S> IntoStatic for PutRecordOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = PutRecordOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        PutRecordOutput {
            cid: self.cid.into_static(),
            commit: self.commit.into_static(),
            uri: self.uri.into_static(),
        }
    }
}

/// Error type for com.atproto.repo.putRecord. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(tag = "error", content = "message")]
pub enum PutRecordError {
    #[serde(rename = "InvalidSwap")]
    InvalidSwap(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(untagged)]
    Other {
        error: SmolStr,
        message: Option<SmolStr>,
    },
}

impl Display for PutRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSwap(msg) => {
                write!(f, "InvalidSwap")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other { error, message } => {
                write!(f, "{}", error)?;
                if let Some(msg) = message {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for PutRecordError {}

/// Response marker for com.atproto.repo.putRecord.
pub struct PutRecordResponse;

impl XrpcResp for PutRecordResponse {
    const NSID: &'static str = "com.atproto.repo.putRecord";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = PutRecordOutput<S>;
    type Err = PutRecordError;
}

impl<S> XrpcRequest for PutRecord<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.repo.putRecord";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Response = PutRecordResponse;
}

// ============================================================================
// com.atproto.repo.deleteRecord
// ============================================================================

/// Request for com.atproto.repo.deleteRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRecord<S: BosStr = DefaultStr> {
    pub collection: Nsid<S>,
    pub repo: AtIdentifier<S>,
    pub rkey: RecordKey<Rkey<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_commit: Option<Cid<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_record: Option<Cid<S>>,
}

impl<S> IntoStatic for DeleteRecord<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = DeleteRecord<S::Output>;

    fn into_static(self) -> Self::Output {
        DeleteRecord {
            collection: self.collection.into_static(),
            repo: self.repo.into_static(),
            rkey: self.rkey.into_static(),
            swap_commit: self.swap_commit.into_static(),
            swap_record: self.swap_record.into_static(),
        }
    }
}

/// Output for com.atproto.repo.deleteRecord. Empty on success apart from
/// optional commit metadata.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRecordOutput<S: BosStr = DefaultStr> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitMeta<S>>,
}

impl<S> IntoStatic for DeleteRecordOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = DeleteRecordOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        DeleteRecordOutput {
            commit: self.commit.into_static(),
        }
    }
}

/// Error type for com.atproto.repo.deleteRecord. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(tag = "error", content = "message")]
pub enum DeleteRecordError {
    #[serde(rename = "InvalidSwap")]
    InvalidSwap(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(untagged)]
    Other {
        error: SmolStr,
        message: Option<SmolStr>,
    },
}

impl Display for DeleteRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSwap(msg) => {
                write!(f, "InvalidSwap")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other { error, message } => {
                write!(f, "{}", error)?;
                if let Some(msg) = message {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for DeleteRecordError {}

/// Response marker for com.atproto.repo.deleteRecord.
pub struct DeleteRecordResponse;

impl XrpcResp for DeleteRecordResponse {
    const NSID: &'static str = "com.atproto.repo.deleteRecord";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = DeleteRecordOutput<S>;
    type Err = DeleteRecordError;
}

impl<S> XrpcRequest for DeleteRecord<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.repo.deleteRecord";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Response = DeleteRecordResponse;
}

// ============================================================================
// com.atproto.repo.uploadBlob
// ============================================================================

/// Request for com.atproto.repo.uploadBlob. The body is raw bytes, not JSON.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct UploadBlob {
    pub body: Bytes,
}

/// Output for com.atproto.repo.uploadBlob.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct UploadBlobOutput<S: BosStr = DefaultStr> {
    pub blob: BlobRef<S>,
}

impl<S> IntoStatic for UploadBlobOutput<S>
where
    S: BosStr + AsRef<str> + IntoStatic,
    S::Output: BosStr + AsRef<str>,
{
    type Output = UploadBlobOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        UploadBlobOutput {
            blob: self.blob.into_static(),
        }
    }
}

/// Response marker for com.atproto.repo.uploadBlob.
pub struct UploadBlobResponse;

impl XrpcResp for UploadBlobResponse {
    const NSID: &'static str = "com.atproto.repo.uploadBlob";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = UploadBlobOutput<S>;
    type Err = GenericError;
}

impl XrpcRequest for UploadBlob {
    const NSID: &'static str = "com.atproto.repo.uploadBlob";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("*/*");
    type Response = UploadBlobResponse;

    /// Raw bytes are appended directly to the buffer without JSON encoding.
    fn encode_body(&self, buffer: &mut Vec<u8>) -> Result<(), EncodeError>
    where
        Self: Serialize,
    {
        buffer.extend_from_slice(self.body.as_ref());
        Ok(())
    }

    /// Decode raw bytes from the request body, copying into a `Bytes`.
    fn decode_body<'de>(body: &'de [u8]) -> Result<Self, DecodeError>
    where
        Self: Deserialize<'de>,
    {
        Ok(Self {
            body: Bytes::copy_from_slice(body),
        })
    }
}

// ============================================================================
// com.atproto.identity.resolveHandle
// ============================================================================

/// Request for com.atproto.identity.resolveHandle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveHandle<S: BosStr = DefaultStr> {
    pub handle: Handle<S>,
}

impl<S> IntoStatic for ResolveHandle<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ResolveHandle<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ResolveHandle {
            handle: self.handle.into_static(),
        }
    }
}

/// Output for com.atproto.identity.resolveHandle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveHandleOutput<S: BosStr = DefaultStr> {
    pub did: Did<S>,
}

impl<S> IntoStatic for ResolveHandleOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ResolveHandleOutput<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ResolveHandleOutput {
            did: self.did.into_static(),
        }
    }
}

/// Error type for com.atproto.identity.resolveHandle. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "error", content = "message")]
#[allow(missing_docs)]
pub enum ResolveHandleError {
    #[serde(rename = "HandleNotFound")]
    HandleNotFound(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(other)]
    Other,
}

impl Display for ResolveHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandleNotFound(msg) => {
                write!(f, "HandleNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other => write!(f, "Unknown error"),
        }
    }
}

impl Error for ResolveHandleError {}

/// Response marker for com.atproto.identity.resolveHandle.
pub struct ResolveHandleResponse;

impl XrpcResp for ResolveHandleResponse {
    const NSID: &'static str = "com.atproto.identity.resolveHandle";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = ResolveHandleOutput<S>;
    type Err = ResolveHandleError;
}

impl<S: BosStr + Serialize> XrpcRequest for ResolveHandle<S> {
    const NSID: &'static str = "com.atproto.identity.resolveHandle";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ResolveHandleResponse;
}

// ============================================================================
// com.atproto.identity.resolveDid
// ============================================================================

/// Request for com.atproto.identity.resolveDid.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveDid<S: BosStr = DefaultStr> {
    pub did: Did<S>,
}

impl<S> IntoStatic for ResolveDid<S>
where
    S: BosStr + IntoStatic,
    <S as IntoStatic>::Output: BosStr,
{
    type Output = ResolveDid<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ResolveDid {
            did: self.did.into_static(),
        }
    }
}

/// Output for com.atproto.identity.resolveDid.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveDidOutput<S = DefaultStr>
where
    S: BosStr,
{
    pub did_doc: Data<S>,
}

impl<S> IntoStatic for ResolveDidOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = ResolveDidOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        ResolveDidOutput {
            did_doc: self.did_doc.into_static(),
        }
    }
}

/// Error type for com.atproto.identity.resolveDid. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "error", content = "message")]
#[allow(missing_docs)]
pub enum ResolveDidError {
    #[serde(rename = "DidNotFound")]
    DidNotFound(Option<SmolStr>),
    #[serde(rename = "DidDeactivated")]
    DidDeactivated(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(other)]
    Other,
}

impl Display for ResolveDidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DidNotFound(msg) => {
                write!(f, "DidNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::DidDeactivated(msg) => {
                write!(f, "DidDeactivated")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other => write!(f, "Unknown error"),
        }
    }
}

impl Error for ResolveDidError {}

/// Response marker for com.atproto.identity.resolveDid.
pub struct ResolveDidResponse;

impl XrpcResp for ResolveDidResponse {
    const NSID: &'static str = "com.atproto.identity.resolveDid";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = ResolveDidOutput<S>;
    type Err = ResolveDidError;
}

impl<S> XrpcRequest for ResolveDid<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.identity.resolveDid";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ResolveDidResponse;
}

// ============================================================================
// com.atproto.server.createSession
// ============================================================================

/// Request for com.atproto.server.createSession.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct CreateSession<S: BosStr = DefaultStr> {
    /// Handle or other identifier supported by the server for the authenticating user.
    pub identifier: S,
    pub password: S,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_factor_token: Option<S>,
    /// When true, instead of throwing an error for takendown accounts, a valid
    /// response with a narrow scoped token is returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_takendown: Option<bool>,
}

impl<S> IntoStatic for CreateSession<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = CreateSession<S::Output>;

    fn into_static(self) -> Self::Output {
        CreateSession {
            identifier: self.identifier.into_static(),
            password: self.password.into_static(),
            auth_factor_token: self.auth_factor_token.into_static(),
            allow_takendown: self.allow_takendown,
        }
    }
}

/// Output for com.atproto.server.createSession.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct CreateSessionOutput<S = DefaultStr>
where
    S: BosStr,
{
    pub access_jwt: S,
    pub refresh_jwt: S,
    pub handle: Handle<S>,
    pub did: Did<S>,
    /// If present, a DID document to use for resolution instead of performing
    /// network lookups for the returned DID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_doc: Option<Data<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_confirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_auth_factor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// If `active` is `false`, this optional field indicates a possible reason
    /// for why the account is not active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<S>,
}

impl<S> IntoStatic for CreateSessionOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = CreateSessionOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        CreateSessionOutput {
            access_jwt: self.access_jwt.into_static(),
            refresh_jwt: self.refresh_jwt.into_static(),
            handle: self.handle.into_static(),
            did: self.did.into_static(),
            did_doc: self.did_doc.into_static(),
            email: self.email.into_static(),
            email_confirmed: self.email_confirmed,
            email_auth_factor: self.email_auth_factor,
            active: self.active,
            status: self.status.into_static(),
        }
    }
}

/// Error type for com.atproto.server.createSession. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "error", content = "message")]
#[allow(missing_docs)]
pub enum CreateSessionError {
    #[serde(rename = "AccountTakedown")]
    AccountTakedown(Option<SmolStr>),
    #[serde(rename = "AuthFactorTokenRequired")]
    AuthFactorTokenRequired(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(untagged)]
    Other {
        error: SmolStr,
        message: Option<SmolStr>,
    },
}

impl Display for CreateSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountTakedown(msg) => {
                write!(f, "AccountTakedown")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::AuthFactorTokenRequired(msg) => {
                write!(f, "AuthFactorTokenRequired")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other { error, message } => {
                write!(f, "{}", error)?;
                if let Some(msg) = message {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for CreateSessionError {}

/// Response marker for com.atproto.server.createSession.
pub struct CreateSessionResponse;

impl XrpcResp for CreateSessionResponse {
    const NSID: &'static str = "com.atproto.server.createSession";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = CreateSessionOutput<S>;
    type Err = CreateSessionError;
}

impl<S> XrpcRequest for CreateSession<S>
where
    S: BosStr + Serialize,
{
    const NSID: &'static str = "com.atproto.server.createSession";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Response = CreateSessionResponse;
}

// ============================================================================
// com.atproto.server.getSession
// ============================================================================

/// Output for com.atproto.server.getSession.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct GetSessionOutput<S = DefaultStr>
where
    S: BosStr,
{
    pub handle: Handle<S>,
    pub did: Did<S>,
    /// If present, a DID document to use for resolution instead of performing
    /// network lookups for the returned DID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_doc: Option<DidDocument<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_confirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_auth_factor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// If `active` is `false`, this optional field indicates a possible reason
    /// for why the account is not active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<S>,
}

impl<S> IntoStatic for GetSessionOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = GetSessionOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        GetSessionOutput {
            handle: self.handle.into_static(),
            did: self.did.into_static(),
            did_doc: self.did_doc.into_static(),
            email: self.email.into_static(),
            email_confirmed: self.email_confirmed,
            email_auth_factor: self.email_auth_factor,
            active: self.active,
            status: self.status.into_static(),
        }
    }
}

/// Response marker for com.atproto.server.getSession. Uses `GenericError` since
/// the generated lexicon has no dedicated error type for this endpoint.
pub struct GetSessionResponse;

impl XrpcResp for GetSessionResponse {
    const NSID: &'static str = "com.atproto.server.getSession";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = GetSessionOutput<S>;
    type Err = GenericError;
}

/// Request marker for com.atproto.server.getSession.
///
/// This endpoint takes no request parameters or body.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub struct GetSession;

impl XrpcRequest for GetSession {
    const NSID: &'static str = "com.atproto.server.getSession";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = GetSessionResponse;
}

// ============================================================================
// com.atproto.server.refreshSession
// ============================================================================

/// Output for com.atproto.server.refreshSession.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct RefreshSessionOutput<S = DefaultStr>
where
    S: BosStr,
{
    pub access_jwt: S,
    pub refresh_jwt: S,
    pub handle: Handle<S>,
    pub did: Did<S>,
    /// If present, a DID document to use for resolution instead of performing
    /// network lookups for the returned DID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_doc: Option<Data<S>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_confirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_auth_factor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// Hosting status of the account. If not specified, assume `active`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<S>,
}

impl<S> IntoStatic for RefreshSessionOutput<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = RefreshSessionOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        RefreshSessionOutput {
            access_jwt: self.access_jwt.into_static(),
            refresh_jwt: self.refresh_jwt.into_static(),
            handle: self.handle.into_static(),
            did: self.did.into_static(),
            did_doc: self.did_doc.into_static(),
            email: self.email.into_static(),
            email_confirmed: self.email_confirmed,
            email_auth_factor: self.email_auth_factor,
            active: self.active,
            status: self.status.into_static(),
        }
    }
}

/// Error type for com.atproto.server.refreshSession. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "error", content = "message")]
#[allow(missing_docs)]
pub enum RefreshSessionError {
    #[serde(rename = "AccountTakedown")]
    AccountTakedown(Option<SmolStr>),
    #[serde(rename = "InvalidToken")]
    InvalidToken(Option<SmolStr>),
    #[serde(rename = "ExpiredToken")]
    ExpiredToken(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(untagged)]
    Other {
        error: SmolStr,
        message: Option<SmolStr>,
    },
}

impl Display for RefreshSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountTakedown(msg) => {
                write!(f, "AccountTakedown")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::InvalidToken(msg) => {
                write!(f, "InvalidToken")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::ExpiredToken(msg) => {
                write!(f, "ExpiredToken")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other { error, message } => {
                write!(f, "{}", error)?;
                if let Some(msg) = message {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for RefreshSessionError {}

/// Response marker for com.atproto.server.refreshSession.
pub struct RefreshSessionResponse;

impl XrpcResp for RefreshSessionResponse {
    const NSID: &'static str = "com.atproto.server.refreshSession";
    const ENCODING: &'static str = "application/json";
    type Output<S: BosStr> = RefreshSessionOutput<S>;
    type Err = RefreshSessionError;
}

/// Request marker for com.atproto.server.refreshSession.
///
/// This endpoint takes no request parameters or body; the refresh token is
/// supplied via the HTTP `Authorization` header.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub struct RefreshSession;

impl XrpcRequest for RefreshSession {
    const NSID: &'static str = "com.atproto.server.refreshSession";
    const METHOD: XrpcMethod = XrpcMethod::Procedure("application/json");
    type Response = RefreshSessionResponse;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntoStatic, cowstr::ToCowStr, types::value::Object};
    use alloc::collections::BTreeMap;

    #[test]
    fn test_list_records_serializes() {
        let req = ListRecords {
            repo: AtIdentifier::new("test.bsky.social".to_cowstr()).unwrap(),
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            cursor: None,
            limit: Some(50),
            reverse: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["repo"], "test.bsky.social");
        assert_eq!(json["collection"], "app.bsky.feed.post");
        assert_eq!(json["limit"], 50);
        assert!(!json.as_object().unwrap().contains_key("cursor"));
    }

    #[test]
    fn test_list_records_output_deserializes() {
        let json_str = r#"{
            "records": [
                {
                    "uri": "at://did:plc:test/app.bsky.feed.post/123",
                    "cid": "bafy123",
                    "value": {}
                }
            ],
            "cursor": "page2"
        }"#;

        let output: ListRecordsOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.records.len(), 1);
        assert!(output.cursor.is_some());
        assert_eq!(output.cursor.as_ref().unwrap().as_str(), "page2");
    }

    #[test]
    fn test_get_record_output_deserializes() {
        let json_str = r#"{
            "uri": "at://did:plc:test/app.bsky.feed.post/123",
            "cid": "bafy123",
            "value": {}
        }"#;

        let output: GetRecordOutput = serde_json::from_str(json_str).unwrap();
        assert!(output.cid.is_some());
        assert_eq!(output.cid.as_ref().unwrap().as_str(), "bafy123");
    }

    #[test]
    fn test_get_record_error_deserializes() {
        let json_str = r#"{"error": "RecordNotFound", "message": "not found"}"#;
        let error: GetRecordError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, GetRecordError::RecordNotFound(Some(_))));
    }

    #[test]
    fn test_resolve_handle_output_deserializes() {
        let json_str = r#"{"did": "did:plc:abc123"}"#;
        let output: ResolveHandleOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.did.as_str(), "did:plc:abc123");
    }

    #[test]
    fn test_resolve_handle_error_deserializes() {
        let json_str = r#"{"error": "HandleNotFound", "message": "handle not found"}"#;
        let error: ResolveHandleError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, ResolveHandleError::HandleNotFound(Some(_))));
    }

    #[test]
    fn test_resolve_did_output_deserializes() {
        let json_str = r#"{"didDoc": {}}"#;
        let output: ResolveDidOutput = serde_json::from_str(json_str).unwrap();
        let _ = output;
    }

    #[test]
    fn test_resolve_did_error_deserializes_not_found() {
        let json_str = r#"{"error": "DidNotFound", "message": "did not found"}"#;
        let error: ResolveDidError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, ResolveDidError::DidNotFound(Some(_))));
    }

    #[test]
    fn test_resolve_did_error_deserializes_deactivated() {
        let json_str = r#"{"error": "DidDeactivated", "message": "did is deactivated"}"#;
        let error: ResolveDidError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, ResolveDidError::DidDeactivated(Some(_))));
    }

    #[test]
    fn test_types_implement_into_static() {
        let list_records = ListRecords {
            repo: AtIdentifier::new("test.bsky.social".to_cowstr()).unwrap(),
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            cursor: None,
            limit: Some(50),
            reverse: None,
        };
        let _static = list_records.into_static();

        let get_record = GetRecord {
            repo: AtIdentifier::new("test.bsky.social".to_cowstr()).unwrap(),
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            rkey: RecordKey::any_owned("abc123").unwrap(),
            cid: None,
        };
        let _static = get_record.into_static();

        let resolve_handle = ResolveHandle {
            handle: Handle::new("test.bsky.social").unwrap().into_static(),
        };
        let _static = resolve_handle.into_static();

        let resolve_did = ResolveDid {
            did: Did::new("did:plc:abc123").unwrap().into_static(),
        };
        let _static = resolve_did.into_static();

        let create_record = CreateRecord {
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            record: Data::Object(Object(BTreeMap::new())),
            repo: AtIdentifier::new("did:plc:abc123".to_cowstr()).unwrap(),
            rkey: None,
            swap_commit: None,
            validate: None,
        };
        let _static = create_record.into_static();

        let put_record = PutRecord {
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            record: Data::Object(Object(BTreeMap::new())),
            repo: AtIdentifier::new("did:plc:abc123".to_cowstr()).unwrap(),
            rkey: RecordKey::any_owned("abc123").unwrap(),
            swap_commit: None,
            swap_record: None,
            validate: None,
        };
        let _static = put_record.into_static();

        let delete_record = DeleteRecord {
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            repo: AtIdentifier::new("did:plc:abc123".to_cowstr()).unwrap(),
            rkey: RecordKey::any_owned("abc123").unwrap(),
            swap_commit: None,
            swap_record: None,
        };
        let _static = delete_record.into_static();

        let commit_meta: CommitMeta =
            serde_json::from_str(r#"{"cid": "bafy123", "rev": "3zzsbxqyly42y"}"#).unwrap();
        let _static = commit_meta.into_static();
    }

    #[test]
    fn test_create_record_serializes() {
        let req = CreateRecord {
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            record: Data::Object(Object(BTreeMap::new())),
            repo: AtIdentifier::new("did:plc:abc123".to_cowstr()).unwrap(),
            rkey: None,
            swap_commit: None,
            validate: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["collection"], "app.bsky.feed.post");
        assert_eq!(json["repo"], "did:plc:abc123");
        // Optional fields should be skipped when None.
        assert!(!json.as_object().unwrap().contains_key("rkey"));
        assert!(!json.as_object().unwrap().contains_key("swapCommit"));
        assert!(!json.as_object().unwrap().contains_key("validate"));
    }

    #[test]
    fn test_create_record_output_deserializes() {
        let json_str = r#"{
            "uri": "at://did:plc:test/app.bsky.feed.post/123",
            "cid": "bafy123"
        }"#;

        let output: CreateRecordOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.cid.as_str(), "bafy123");
        assert!(output.commit.is_none());
    }

    #[test]
    fn test_create_record_output_deserializes_with_commit() {
        let json_str = r#"{
            "uri": "at://did:plc:test/app.bsky.feed.post/123",
            "cid": "bafy123",
            "commit": {
                "cid": "bafycommit",
                "rev": "3zzsbxqyly42y"
            }
        }"#;

        let output: CreateRecordOutput = serde_json::from_str(json_str).unwrap();
        let commit = output.commit.unwrap();
        assert_eq!(commit.cid.as_str(), "bafycommit");
        assert_eq!(commit.rev.as_str(), "3zzsbxqyly42y");
    }

    #[test]
    fn test_create_record_error_deserializes_invalid_swap() {
        let json_str = r#"{"error": "InvalidSwap", "message": "bad swap"}"#;
        let error: CreateRecordError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, CreateRecordError::InvalidSwap(Some(_))));
    }

    #[test]
    fn test_create_record_error_deserializes_other() {
        let json_str = r#"{"error": "RepoNotFound", "message": "no such repo"}"#;
        let error: CreateRecordError = serde_json::from_str(json_str).unwrap();
        match error {
            CreateRecordError::Other { error, message } => {
                assert_eq!(error, "RepoNotFound");
                assert_eq!(message.as_deref(), Some("no such repo"));
            }
            other => panic!("expected Other variant, got {:?}", other),
        }
    }

    #[test]
    fn test_put_record_serializes() {
        let req = PutRecord {
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            record: Data::Object(Object(BTreeMap::new())),
            repo: AtIdentifier::new("did:plc:abc123".to_cowstr()).unwrap(),
            rkey: RecordKey::any_owned("abc123").unwrap(),
            swap_commit: None,
            swap_record: None,
            validate: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["collection"], "app.bsky.feed.post");
        assert_eq!(json["repo"], "did:plc:abc123");
        assert_eq!(json["rkey"], "abc123");
    }

    #[test]
    fn test_put_record_output_deserializes() {
        let json_str = r#"{
            "uri": "at://did:plc:test/app.bsky.feed.post/123",
            "cid": "bafy123"
        }"#;

        let output: PutRecordOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.cid.as_str(), "bafy123");
    }

    #[test]
    fn test_put_record_error_deserializes() {
        let json_str = r#"{"error": "InvalidSwap", "message": "bad swap"}"#;
        let error: PutRecordError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, PutRecordError::InvalidSwap(Some(_))));
    }

    #[test]
    fn test_delete_record_serializes() {
        let req = DeleteRecord {
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            repo: AtIdentifier::new("did:plc:abc123".to_cowstr()).unwrap(),
            rkey: RecordKey::any_owned("abc123").unwrap(),
            swap_commit: None,
            swap_record: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["collection"], "app.bsky.feed.post");
        assert_eq!(json["repo"], "did:plc:abc123");
        assert_eq!(json["rkey"], "abc123");
    }

    #[test]
    fn test_delete_record_output_deserializes_empty() {
        let json_str = "{}";
        let output: DeleteRecordOutput = serde_json::from_str(json_str).unwrap();
        assert!(output.commit.is_none());
    }

    #[test]
    fn test_delete_record_output_deserializes_with_commit() {
        let json_str = r#"{
            "commit": {
                "cid": "bafycommit",
                "rev": "3zzsbxqyly42y"
            }
        }"#;

        let output: DeleteRecordOutput = serde_json::from_str(json_str).unwrap();
        let commit = output.commit.unwrap();
        assert_eq!(commit.cid.as_str(), "bafycommit");
    }

    #[test]
    fn test_delete_record_error_deserializes() {
        let json_str = r#"{"error": "InvalidSwap", "message": "bad swap"}"#;
        let error: DeleteRecordError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, DeleteRecordError::InvalidSwap(Some(_))));
    }

    #[test]
    fn test_upload_blob_round_trips_body() {
        let req = UploadBlob {
            body: Bytes::from_static(b"hello world"),
        };

        // The body is raw bytes, not JSON. Encode then decode.
        let mut buffer = Vec::new();
        UploadBlob::encode_body(&req, &mut buffer).unwrap();
        assert_eq!(&buffer, b"hello world");

        let decoded = UploadBlob::decode_body(&buffer).unwrap();
        assert_eq!(decoded.body.as_ref(), b"hello world");
    }

    #[test]
    fn test_upload_blob_output_deserializes() {
        let json_str = r#"{
            "blob": {
                "$type": "blob",
                "ref": {"$link": "bafy123"},
                "mimeType": "image/png",
                "size": 12345
            }
        }"#;

        let output: UploadBlobOutput = serde_json::from_str(json_str).unwrap();
        let blob = output.blob.blob();
        assert_eq!(blob.size, 12345);
    }

    #[test]
    fn test_create_session_serializes() {
        let req = CreateSession {
            identifier: SmolStr::new("test.bsky.social"),
            password: SmolStr::new("hunter2"),
            auth_factor_token: None,
            allow_takendown: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["identifier"], "test.bsky.social");
        assert_eq!(json["password"], "hunter2");
        // Optional fields should be skipped when None.
        assert!(!json.as_object().unwrap().contains_key("authFactorToken"));
        assert!(!json.as_object().unwrap().contains_key("allowTakendown"));
    }

    #[test]
    fn test_create_session_serializes_with_optional_fields() {
        let req = CreateSession {
            identifier: SmolStr::new("test.bsky.social"),
            password: SmolStr::new("hunter2"),
            auth_factor_token: Some(SmolStr::new("123456")),
            allow_takendown: Some(true),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["authFactorToken"], "123456");
        assert_eq!(json["allowTakendown"], true);
    }

    #[test]
    fn test_create_session_output_deserializes() {
        let json_str = r#"{
            "accessJwt": "access-token",
            "refreshJwt": "refresh-token",
            "handle": "test.bsky.social",
            "did": "did:plc:abc123",
            "email": "test@example.com",
            "emailConfirmed": true,
            "active": true
        }"#;

        let output: CreateSessionOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.access_jwt.as_str(), "access-token");
        assert_eq!(output.refresh_jwt.as_str(), "refresh-token");
        assert_eq!(output.handle.as_str(), "test.bsky.social");
        assert_eq!(output.did.as_str(), "did:plc:abc123");
        assert_eq!(output.email.as_ref().unwrap().as_str(), "test@example.com");
        assert_eq!(output.email_confirmed, Some(true));
        assert_eq!(output.active, Some(true));
        // Optional fields absent from the payload remain None.
        assert!(output.did_doc.is_none());
        assert!(output.email_auth_factor.is_none());
        assert!(output.status.is_none());
    }

    #[test]
    fn test_create_session_output_deserializes_with_status_and_did_doc() {
        let json_str = r#"{
            "accessJwt": "access-token",
            "refreshJwt": "refresh-token",
            "handle": "test.bsky.social",
            "did": "did:plc:abc123",
            "didDoc": {"@context": ["https://www.w3.org/ns/did/v1"]},
            "status": "takendown",
            "active": false
        }"#;

        let output: CreateSessionOutput = serde_json::from_str(json_str).unwrap();
        assert!(output.did_doc.is_some());
        assert_eq!(output.active, Some(false));
        assert_eq!(output.status.as_ref().unwrap().as_str(), "takendown");
    }

    #[test]
    fn test_create_session_output_into_static() {
        let output = CreateSessionOutput {
            access_jwt: SmolStr::new("access-token"),
            refresh_jwt: SmolStr::new("refresh-token"),
            handle: Handle::new("test.bsky.social").unwrap().into_static(),
            did: Did::new("did:plc:abc123").unwrap().into_static(),
            did_doc: None,
            email: Some(SmolStr::new("test@example.com")),
            email_confirmed: Some(true),
            email_auth_factor: None,
            active: Some(true),
            status: Some(SmolStr::new("active")),
        };
        let static_output: CreateSessionOutput = output.into_static();
        assert_eq!(static_output.access_jwt.as_str(), "access-token");
        assert_eq!(
            static_output.email.as_ref().unwrap().as_str(),
            "test@example.com"
        );
    }

    #[test]
    fn test_create_session_error_account_takedown() {
        let json_str = r#"{"error": "AccountTakedown", "message": "taken down"}"#;
        let error: CreateSessionError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(
            error,
            CreateSessionError::AccountTakedown(Some(_))
        ));
    }

    #[test]
    fn test_create_session_error_auth_factor_required() {
        let json_str = r#"{"error": "AuthFactorTokenRequired", "message": "2fa required"}"#;
        let error: CreateSessionError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(
            error,
            CreateSessionError::AuthFactorTokenRequired(Some(_))
        ));
    }

    #[test]
    fn test_create_session_error_other_catch_all() {
        // An unknown error code should fall into the untagged Other variant.
        let json_str = r#"{"error": "SomeUnknownCode", "message": "weird"}"#;
        let error: CreateSessionError = serde_json::from_str(json_str).unwrap();
        match error {
            CreateSessionError::Other { error, message } => {
                assert_eq!(error.as_str(), "SomeUnknownCode");
                assert_eq!(message.unwrap().as_str(), "weird");
            }
            other => panic!("expected Other, got {:?}", other),
        }
    }

    #[test]
    fn test_get_session_output_deserializes() {
        let json_str = r#"{
            "handle": "test.bsky.social",
            "did": "did:plc:abc123",
            "email": "test@example.com",
            "emailConfirmed": true,
            "active": true
        }"#;

        let output: GetSessionOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.handle.as_str(), "test.bsky.social");
        assert_eq!(output.did.as_str(), "did:plc:abc123");
        assert_eq!(output.email_confirmed, Some(true));
        assert_eq!(output.active, Some(true));
    }

    #[test]
    fn test_get_session_output_into_static() {
        let output = GetSessionOutput {
            handle: Handle::new("test.bsky.social").unwrap().into_static(),
            did: Did::new("did:plc:abc123").unwrap().into_static(),
            did_doc: None,
            email: Some(SmolStr::new("test@example.com")),
            email_confirmed: Some(true),
            email_auth_factor: None,
            active: Some(true),
            status: None,
        };
        let static_output: GetSessionOutput = output.into_static();
        assert_eq!(static_output.handle.as_str(), "test.bsky.social");
    }

    #[test]
    fn test_refresh_session_output_deserializes() {
        let json_str = r#"{
            "accessJwt": "new-access",
            "refreshJwt": "new-refresh",
            "handle": "test.bsky.social",
            "did": "did:plc:abc123",
            "active": true
        }"#;

        let output: RefreshSessionOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.access_jwt.as_str(), "new-access");
        assert_eq!(output.refresh_jwt.as_str(), "new-refresh");
        assert_eq!(output.did.as_str(), "did:plc:abc123");
        assert_eq!(output.active, Some(true));
    }

    #[test]
    fn test_refresh_session_output_into_static() {
        let output = RefreshSessionOutput {
            access_jwt: SmolStr::new("new-access"),
            refresh_jwt: SmolStr::new("new-refresh"),
            handle: Handle::new("test.bsky.social").unwrap().into_static(),
            did: Did::new("did:plc:abc123").unwrap().into_static(),
            did_doc: None,
            email: None,
            email_confirmed: None,
            email_auth_factor: None,
            active: Some(true),
            status: None,
        };
        let static_output: RefreshSessionOutput = output.into_static();
        assert_eq!(static_output.access_jwt.as_str(), "new-access");
    }

    #[test]
    fn test_refresh_session_error_invalid_token() {
        let json_str = r#"{"error": "InvalidToken", "message": "bad token"}"#;
        let error: RefreshSessionError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, RefreshSessionError::InvalidToken(Some(_))));
    }

    #[test]
    fn test_refresh_session_error_expired_token() {
        let json_str = r#"{"error": "ExpiredToken"}"#;
        let error: RefreshSessionError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, RefreshSessionError::ExpiredToken(None)));
    }

    #[test]
    fn test_refresh_session_error_other_catch_all() {
        let json_str = r#"{"error": "SomethingElse", "message": "hmm"}"#;
        let error: RefreshSessionError = serde_json::from_str(json_str).unwrap();
        match error {
            RefreshSessionError::Other { error, message } => {
                assert_eq!(error.as_str(), "SomethingElse");
                assert_eq!(message.unwrap().as_str(), "hmm");
            }
            other => panic!("expected Other, got {:?}", other),
        }
    }
}
