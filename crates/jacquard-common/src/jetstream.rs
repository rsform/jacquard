//! Jetstream subscription support
//!
//! Jetstream is a simplified JSON-based alternative to the atproto firehose.
//! Unlike subscribeRepos which uses DAG-CBOR, Jetstream uses JSON encoding.

use crate::types::string::{Datetime, Did, Handle};
use crate::xrpc::{MessageEncoding, SubscriptionResp, XrpcSubscription};
use crate::{CowStr, Data, IntoStatic};
use serde::{Deserialize, Serialize};

/// Parameters for subscribing to Jetstream
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, bon::Builder)]
#[serde(rename_all = "camelCase")]
#[builder(start_fn = new)]
pub struct JetstreamParams<'a> {
    /// Filter by collection NSIDs (max 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    #[builder(into)]
    pub wanted_collections: Option<Vec<crate::CowStr<'a>>>,

    /// Filter by DIDs (max 10,000)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    #[builder(into)]
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

/// Commit event details
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JetstreamCommit<'a> {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub record: Option<Data<'a>>,
    /// Content identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cid: Option<CowStr<'a>>,
}

/// Identity event details
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JetstreamIdentity<'a> {
    /// DID
    #[serde(borrow)]
    pub did: Did<'a>,
    /// Handle
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub handle: Option<Handle<'a>>,
    /// Sequence number
    pub seq: i64,
    /// Timestamp
    pub time: Datetime,
}

/// Account event details
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JetstreamAccount<'a> {
    /// Account active status
    pub active: bool,
    /// DID
    #[serde(borrow)]
    pub did: Did<'a>,
    /// Sequence number
    pub seq: i64,
    /// Timestamp
    pub time: Datetime,
    /// Optional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub status: Option<CowStr<'a>>,
}

/// Jetstream event message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
#[serde(rename_all = "lowercase")]
pub enum JetstreamMessage<'a> {
    /// Commit event
    Commit {
        /// DID
        #[serde(borrow)]
        did: Did<'a>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Commit details
        #[serde(borrow)]
        commit: JetstreamCommit<'a>,
    },
    /// Identity event
    Identity {
        /// DID
        #[serde(borrow)]
        did: Did<'a>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Identity details
        #[serde(borrow)]
        identity: JetstreamIdentity<'a>,
    },
    /// Account event
    Account {
        /// DID
        #[serde(borrow)]
        did: Did<'a>,
        /// Unix microseconds timestamp
        time_us: i64,
        /// Account details
        #[serde(borrow)]
        account: JetstreamAccount<'a>,
    },
}

impl IntoStatic for CommitOperation {
    type Output = CommitOperation;

    fn into_static(self) -> Self::Output {
        self
    }
}

impl IntoStatic for JetstreamCommit<'_> {
    type Output = JetstreamCommit<'static>;

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

impl IntoStatic for JetstreamIdentity<'_> {
    type Output = JetstreamIdentity<'static>;

    fn into_static(self) -> Self::Output {
        JetstreamIdentity {
            did: self.did.into_static(),
            handle: self.handle.map(|h| h.into_static()),
            seq: self.seq,
            time: self.time,
        }
    }
}

impl IntoStatic for JetstreamAccount<'_> {
    type Output = JetstreamAccount<'static>;

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

impl IntoStatic for JetstreamMessage<'_> {
    type Output = JetstreamMessage<'static>;

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

/// Stream response type for Jetstream subscriptions
pub struct JetstreamStream;

impl SubscriptionResp for JetstreamStream {
    const NSID: &'static str = "jetstream";
    const ENCODING: MessageEncoding = MessageEncoding::Json;

    /// Typed Jetstream message
    type Message<'de> = JetstreamMessage<'de>;

    /// Generic error type
    type Error<'de> = crate::xrpc::GenericError<'de>;
}

impl<'a> XrpcSubscription for JetstreamParams<'a> {
    const NSID: &'static str = "jetstream";
    const ENCODING: MessageEncoding = MessageEncoding::Json;
    const CUSTOM_PATH: Option<&'static str> = Some("/subscribe");
    type Stream = JetstreamStream;
}

impl IntoStatic for JetstreamParams<'_> {
    type Output = JetstreamParams<'static>;

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
