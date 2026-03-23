//! Jetstream subscription support
//!
//! Jetstream is a simplified JSON-based alternative to the atproto firehose.
//! Unlike subscribeRepos which uses DAG-CBOR, Jetstream uses JSON encoding.

use crate::bos::{BosStr, DefaultStr};
use crate::types::cid::Cid;
use crate::types::nsid::Nsid;
use crate::types::string::{Datetime, Did, Handle, Rkey};
use crate::xrpc::{MessageEncoding, SubscriptionResp, XrpcSubscription};
use crate::{CowStr, Data, IntoStatic, RawData};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Parameters for subscribing to Jetstream
#[cfg_attr(feature = "std", derive(bon::Builder))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "std", builder(start_fn = new))]
pub struct JetstreamParams<S: BosStr = DefaultStr> {
    /// Filter by collection NSIDs (max 100)
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wanted_collections: Option<Vec<Nsid<S>>>,

    /// Filter by DIDs (max 10,000)
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wanted_dids: Option<Vec<Did<S>>>,

    /// Unix microseconds timestamp to start playback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<i64>,

    /// Maximum payload size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_message_size_bytes: Option<u64>,

    /// Enable zstd compression
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<bool>,

    /// Pause stream until first options update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_hello: Option<bool>,
}

/// Commit operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitOperation {
    /// Create a new record
    Create,
    /// Update an existing record
    Update,
    /// Delete a record
    Delete,
}

/// Commit event details (minimal validation)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawJetstreamCommit<'a> {
    /// Revision string
    #[serde(borrow)]
    pub rev: CowStr<'a>,
    /// Operation type
    pub operation: CommitOperation,
    /// Collection NSID
    #[serde(borrow)]
    pub collection: CowStr<'a>,
    /// Record key
    #[serde(borrow)]
    pub rkey: CowStr<'a>,
    /// Record data (present for create/update)
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub record: Option<RawData<'a>>,
    /// Content identifier
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub cid: Option<CowStr<'a>>,
}

/// Commit event details (additional validation)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JetstreamCommit<S: BosStr = DefaultStr> {
    /// Revision string
    pub rev: S,
    /// Operation type
    pub operation: CommitOperation,
    /// Collection NSID
    pub collection: Nsid<S>,
    /// Record key
    pub rkey: Rkey<S>,
    /// Record data (present for create/update)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<Data<S>>,
    /// Content identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<Cid<S>>,
}

/// Identity event details
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JetstreamIdentity<S: BosStr = DefaultStr> {
    /// DID
    pub did: Did<S>,
    /// Handle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<Handle<S>>,
    /// Sequence number
    pub seq: i64,
    /// Timestamp
    pub time: Datetime,
}

/// Account event details
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JetstreamAccount<S: BosStr = DefaultStr> {
    /// Account active status
    pub active: bool,
    /// DID
    pub did: Did<S>,
    /// Sequence number
    pub seq: i64,
    /// Timestamp
    pub time: Datetime,
    /// Optional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<S>,
}

/// Jetstream event message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum JetstreamMessage<S: BosStr = DefaultStr> {
    /// Commit event
    Commit {
        /// DID
        did: Did<S>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Commit details
        commit: JetstreamCommit<S>,
    },
    /// Identity event
    Identity {
        /// DID
        did: Did<S>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Identity details
        identity: JetstreamIdentity<S>,
    },
    /// Account event
    Account {
        /// DID
        did: Did<S>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Account details
        account: JetstreamAccount<S>,
    },
}

/// Jetstream event message (minimal validation — uses raw data types)
///
/// The `identity` and `account` variants use `CowStr<'a>` as their backing string type,
/// while `commit` is backed by `RawJetstreamCommit<'a>` which uses `RawData<'a>` for
/// the record field. Until `RawData` is migrated to BOS type parameters, this type
/// retains a lifetime parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RawJetstreamMessage<'a> {
    /// Commit event
    Commit {
        /// DID
        #[serde(borrow)]
        did: Did<CowStr<'a>>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Commit details
        #[serde(borrow)]
        commit: RawJetstreamCommit<'a>,
    },
    /// Identity event
    Identity {
        /// DID
        #[serde(borrow)]
        did: Did<CowStr<'a>>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Identity details
        #[serde(borrow)]
        identity: JetstreamIdentity<CowStr<'a>>,
    },
    /// Account event
    Account {
        /// DID
        #[serde(borrow)]
        did: Did<CowStr<'a>>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Account details
        #[serde(borrow)]
        account: JetstreamAccount<CowStr<'a>>,
    },
    /// Unknown message type
    #[serde(untagged)]
    Unknown(RawData<'a>),
}

impl IntoStatic for CommitOperation {
    type Output = CommitOperation;

    fn into_static(self) -> Self::Output {
        self
    }
}

impl<S> IntoStatic for JetstreamCommit<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = JetstreamCommit<S::Output>;

    fn into_static(self) -> Self::Output {
        JetstreamCommit {
            rev: self.rev.into_static(),
            operation: self.operation,
            collection: self.collection.into_static(),
            rkey: self.rkey.into_static(),
            record: self.record.map(|r| r.into_static()),
            cid: self.cid.map(|c| c.into_static()),
        }
    }
}

impl IntoStatic for RawJetstreamCommit<'_> {
    type Output = RawJetstreamCommit<'static>;

    fn into_static(self) -> Self::Output {
        RawJetstreamCommit {
            rev: self.rev.into_static(),
            operation: self.operation,
            collection: self.collection.into_static(),
            rkey: self.rkey.into_static(),
            record: self.record.map(|r| r.into_static()),
            cid: self.cid.map(|c| c.into_static()),
        }
    }
}

impl<S> IntoStatic for JetstreamIdentity<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = JetstreamIdentity<S::Output>;

    fn into_static(self) -> Self::Output {
        JetstreamIdentity {
            did: self.did.into_static(),
            handle: self.handle.map(|h| h.into_static()),
            seq: self.seq,
            time: self.time,
        }
    }
}

impl<S> IntoStatic for JetstreamAccount<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = JetstreamAccount<S::Output>;

    fn into_static(self) -> Self::Output {
        JetstreamAccount {
            active: self.active,
            did: self.did.into_static(),
            seq: self.seq,
            time: self.time,
            status: self.status.map(|s| s.into_static()),
        }
    }
}

impl<S> IntoStatic for JetstreamMessage<S>
where
    S: BosStr + IntoStatic,
    S::Output: BosStr,
{
    type Output = JetstreamMessage<S::Output>;

    fn into_static(self) -> Self::Output {
        match self {
            JetstreamMessage::Commit {
                did,
                time_us,
                commit,
            } => JetstreamMessage::Commit {
                did: did.into_static(),
                time_us,
                commit: commit.into_static(),
            },
            JetstreamMessage::Identity {
                did,
                time_us,
                identity,
            } => JetstreamMessage::Identity {
                did: did.into_static(),
                time_us,
                identity: identity.into_static(),
            },
            JetstreamMessage::Account {
                did,
                time_us,
                account,
            } => JetstreamMessage::Account {
                did: did.into_static(),
                time_us,
                account: account.into_static(),
            },
        }
    }
}

impl IntoStatic for RawJetstreamMessage<'_> {
    type Output = RawJetstreamMessage<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            RawJetstreamMessage::Commit {
                did,
                time_us,
                commit,
            } => RawJetstreamMessage::Commit {
                did: did.into_static(),
                time_us,
                commit: commit.into_static(),
            },
            RawJetstreamMessage::Identity {
                did,
                time_us,
                identity,
            } => RawJetstreamMessage::Identity {
                did: did.into_static(),
                time_us,
                identity: identity.into_static(),
            },
            RawJetstreamMessage::Account {
                did,
                time_us,
                account,
            } => RawJetstreamMessage::Account {
                did: did.into_static(),
                time_us,
                account: account.into_static(),
            },
            RawJetstreamMessage::Unknown(data) => RawJetstreamMessage::Unknown(data.into_static()),
        }
    }
}

/// Stream response type for Jetstream subscriptions
pub struct JetstreamStream;

impl SubscriptionResp for JetstreamStream {
    const NSID: &'static str = "jetstream";
    const ENCODING: MessageEncoding = MessageEncoding::Json;

    /// Typed Jetstream message
    type Message<S: BosStr> = JetstreamMessage<S>;

    /// Generic error type
    type Error = crate::xrpc::GenericError;
}

impl<S: BosStr + Serialize> XrpcSubscription for JetstreamParams<S> {
    const NSID: &'static str = "jetstream";
    const ENCODING: MessageEncoding = MessageEncoding::Json;
    const CUSTOM_PATH: Option<&'static str> = Some("/subscribe");
    type Stream = JetstreamStream;
}

impl<S: BosStr + IntoStatic> IntoStatic for JetstreamParams<S>
where
    S::Output: BosStr,
{
    type Output = JetstreamParams<S::Output>;

    fn into_static(self) -> Self::Output {
        JetstreamParams {
            wanted_collections: self
                .wanted_collections
                .map(|v| v.into_iter().map(|s| s.into_static()).collect()),
            wanted_dids: self
                .wanted_dids
                .map(|v| v.into_iter().map(|s| s.into_static()).collect()),
            cursor: self.cursor,
            max_message_size_bytes: self.max_message_size_bytes,
            compress: self.compress,
            require_hello: self.require_hello,
        }
    }
}

/// Parameters for subscribing to Jetstream
#[cfg_attr(feature = "std", derive(bon::Builder))]
#[cfg_attr(feature = "std", builder(start_fn = new))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawJetstreamParams<'a> {
    /// Filter by collection NSIDs (max 100)
    #[builder(into)]
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub wanted_collections: Option<Vec<crate::CowStr<'a>>>,

    /// Filter by DIDs (max 10,000)
    #[builder(into)]
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub wanted_dids: Option<Vec<crate::CowStr<'a>>>,

    /// Unix microseconds timestamp to start playback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<i64>,

    /// Maximum payload size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_message_size_bytes: Option<u64>,

    /// Enable zstd compression
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<bool>,

    /// Pause stream until first options update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_hello: Option<bool>,
}

/// Stream response type for raw (minimally-validated) Jetstream subscriptions
pub struct JetstreamRawStream;

impl SubscriptionResp for JetstreamRawStream {
    const NSID: &'static str = "jetstream";
    const ENCODING: MessageEncoding = MessageEncoding::Json;

    /// Raw Jetstream message
    type Message<S: BosStr> = RawJetstreamMessage<'static>;

    /// Generic error type
    type Error = crate::xrpc::GenericError;
}

impl<'a> XrpcSubscription for RawJetstreamParams<'a> {
    const NSID: &'static str = "jetstream";
    const ENCODING: MessageEncoding = MessageEncoding::Json;
    const CUSTOM_PATH: Option<&'static str> = Some("/subscribe");
    type Stream = JetstreamRawStream;
}

impl IntoStatic for RawJetstreamParams<'_> {
    type Output = RawJetstreamParams<'static>;

    fn into_static(self) -> Self::Output {
        RawJetstreamParams {
            wanted_collections: self
                .wanted_collections
                .map(|v| v.into_iter().map(|s| s.into_static()).collect()),
            wanted_dids: self
                .wanted_dids
                .map(|v| v.into_iter().map(|s| s.into_static()).collect()),
            cursor: self.cursor,
            max_message_size_bytes: self.max_message_size_bytes,
            compress: self.compress,
            require_hello: self.require_hello,
        }
    }
}
