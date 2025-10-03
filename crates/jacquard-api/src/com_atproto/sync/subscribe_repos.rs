///Represents a change to an account's status on a host (eg, PDS or Relay). The semantics of this event are that the status is at the host which emitted the event, not necessarily that at the currently active PDS. Eg, a Relay takedown would emit a takedown with active=false, even if the PDS is still active.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account<'a> {
    pub active: bool,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    pub seq: i64,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub status: std::option::Option<jacquard_common::CowStr<'a>>,
    pub time: jacquard_common::types::string::Datetime,
}
///Represents an update of repository state. Note that empty commits are allowed, which include no repo data changes, but an update to rev and signature.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Commit<'a> {
    #[serde(borrow)]
    pub blobs: Vec<jacquard_common::types::cid::CidLink<'a>>,
    pub blocks: bytes::Bytes,
    #[serde(borrow)]
    pub commit: jacquard_common::types::cid::CidLink<'a>,
    #[serde(borrow)]
    pub ops: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub prev_data: std::option::Option<jacquard_common::types::cid::CidLink<'a>>,
    pub rebase: bool,
    #[serde(borrow)]
    pub repo: jacquard_common::types::string::Did<'a>,
    pub rev: jacquard_common::types::string::Tid,
    pub seq: i64,
    pub since: jacquard_common::types::string::Tid,
    pub time: jacquard_common::types::string::Datetime,
    pub too_big: bool,
}
///Represents a change to an account's identity. Could be an updated handle, signing key, or pds hosting endpoint. Serves as a prod to all downstream services to refresh their identity cache.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Identity<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub handle: std::option::Option<jacquard_common::types::string::Handle<'a>>,
    pub seq: i64,
    pub time: jacquard_common::types::string::Datetime,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Info<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub message: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeReposParams {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub cursor: std::option::Option<i64>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum SubscribeReposMessage<'a> {
    #[serde(rename = "#commit")]
    Commit(Box<jacquard_common::types::value::Data<'a>>),
    #[serde(rename = "#sync")]
    Sync(Box<jacquard_common::types::value::Data<'a>>),
    #[serde(rename = "#identity")]
    Identity(Box<jacquard_common::types::value::Data<'a>>),
    #[serde(rename = "#account")]
    Account(Box<jacquard_common::types::value::Data<'a>>),
    #[serde(rename = "#info")]
    Info(Box<jacquard_common::types::value::Data<'a>>),
}
#[jacquard_derive::open_union]
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    thiserror::Error,
    miette::Diagnostic
)]
#[serde(tag = "error", content = "message")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum SubscribeReposError<'a> {
    #[serde(rename = "FutureCursor")]
    FutureCursor(std::option::Option<String>),
    ///If the consumer of the stream can not keep up with events, and a backlog gets too large, the server will drop the connection.
    #[serde(rename = "ConsumerTooSlow")]
    ConsumerTooSlow(std::option::Option<String>),
}
impl std::fmt::Display for SubscribeReposError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FutureCursor(msg) => {
                write!(f, "FutureCursor")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::ConsumerTooSlow(msg) => {
                write!(f, "ConsumerTooSlow")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
///A repo operation, ie a mutation of a single record.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoOp<'a> {
    #[serde(borrow)]
    pub action: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub cid: jacquard_common::types::cid::CidLink<'a>,
    #[serde(borrow)]
    pub path: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub prev: std::option::Option<jacquard_common::types::cid::CidLink<'a>>,
}
///Updates the repo to a new state, without necessarily including that state on the firehose. Used to recover from broken commit streams, data loss incidents, or in situations where upstream host does not know recent state of the repository.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sync<'a> {
    pub blocks: bytes::Bytes,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub rev: jacquard_common::CowStr<'a>,
    pub seq: i64,
    pub time: jacquard_common::types::string::Datetime,
}
