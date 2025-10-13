use core::fmt;

use serde::{Deserialize, Serialize};

use crate::IntoStatic;
use crate::types::value::Data;
use crate::types::{
    aturi::RepoPath,
    nsid::Nsid,
    recordkey::{RecordKey, RecordKeyType, Rkey},
};
use crate::xrpc::XrpcResp;

/// Trait for a collection of records that can be stored in a repository.
///
/// The records all have the same Lexicon schema.
///
/// Implemented on the record type itself.
pub trait Collection: fmt::Debug + Serialize {
    /// The NSID for the Lexicon that defines the schema of records in this collection.
    const NSID: &'static str;

    /// A marker type implementing [`XrpcResp`] that allows typed deserialization of records
    /// from this collection. Used by [`Agent::get_record`] to return properly typed responses.
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
    fn nsid() -> crate::types::nsid::Nsid<'static> {
        Nsid::new_static(Self::NSID).expect("should be valid NSID")
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
    fn repo_path<'u, T: RecordKeyType>(
        rkey: &'u crate::types::recordkey::RecordKey<T>,
    ) -> RepoPath<'u> {
        RepoPath {
            collection: Self::nsid(),
            rkey: Some(RecordKey::from(Rkey::raw(rkey.as_ref()))),
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
pub enum RecordError<'a> {
    /// The requested record was not found
    #[error("RecordNotFound")]
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(Option<String>),
    /// An unknown error occurred
    #[error("Unknown")]
    #[serde(rename = "Unknown")]
    #[serde(borrow)]
    Unknown(Data<'a>),
}

impl IntoStatic for RecordError<'_> {
    type Output = RecordError<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            RecordError::RecordNotFound(msg) => RecordError::RecordNotFound(msg),
            RecordError::Unknown(data) => RecordError::Unknown(data.into_static()),
        }
    }
}
