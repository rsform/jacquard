use alloc::string::String;
use core::fmt;
use smol_str::SmolStr;

use serde::{Deserialize, Serialize};

use crate::types::value::Data;
use crate::types::{
    aturi::RepoPath,
    nsid::Nsid,
    recordkey::{RecordKey, RecordKeyType},
};
use crate::xrpc::XrpcResp;
use crate::{BorrowOrShare, Bos, IntoStatic};

/// Trait for a collection of records that can be stored in a repository.
///
/// The records all have the same Lexicon schema.
///
/// Implemented on the record type itself.
pub trait Collection: fmt::Debug + Serialize {
    /// The NSID for the Lexicon that defines the schema of records in this collection.
    const NSID: &'static str;

    /// A marker type implementing [`XrpcResp`] that allows typed deserialization of records
    /// from this collection. Used by [`AgentSessionExt::get_record`](https://docs.rs/jacquard/latest/jacquard/client/trait.AgentSessionExt.html) to return properly typed responses.
    type Record: XrpcResp;

    /// Returns the [`Nsid`] for the Lexicon that defines the schema of records in this
    /// collection.
    ///
    /// This is a convenience method that parses [`Self::NSID`].
    ///
    /// # Panics
    ///
    /// Panics if [`Self::NSID`] is not a valid NSID.
    ///
    /// [`Nsid`]: crate::types::string::Nsid
    fn nsid() -> crate::types::nsid::Nsid<&'static str> {
        unsafe { Nsid::unchecked(Self::NSID) }
    }

    /// Returns the repo path for a record in this collection with the given record key.
    ///
    /// Per the [Repo Data Structure v3] specification:
    /// > Repo paths currently have a fixed structure of `<collection>/<record-key>`. This
    /// > means a valid, normalized [`Nsid`], followed by a `/`, followed by a valid
    /// > [`RecordKey`].
    ///
    /// [Repo Data Structure v3]: https://atproto.com/specs/repository#repo-data-structure-v3
    /// [`Nsid`]: crate::types::string::Nsid
    fn repo_path<T>(rkey: &crate::types::recordkey::RecordKey<T>) -> RepoPath<&str>
    where
        T: RecordKeyType + Bos<str> + AsRef<str>,
    {
        RepoPath {
            collection: Self::nsid(),
            // Borrow the record key string with the caller's lifetime via CowStr.
            rkey: Some(
                RecordKey::any(rkey.0.borrow_or_share())
                    .expect("RecordKey implements RecordKeyType, which guarantees a valid rkey"),
            ),
        }
    }
}

/// Generic error type for record retrieval operations.
///
/// Used by generated collection marker types as their error type.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error, miette::Diagnostic,
)]
#[serde(tag = "error", content = "message")]
#[non_exhaustive]
pub enum RecordError {
    /// The requested record was not found
    #[error("RecordNotFound")]
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(Option<String>),
    /// An unknown error occurred
    #[error("Unknown")]
    #[serde(rename = "Unknown")]
    Unknown(Data<SmolStr>),
}

impl IntoStatic for RecordError {
    type Output = RecordError;

    fn into_static(self) -> Self::Output {
        match self {
            RecordError::RecordNotFound(msg) => RecordError::RecordNotFound(msg),
            RecordError::Unknown(data) => RecordError::Unknown(data.into_static()),
        }
    }
}
