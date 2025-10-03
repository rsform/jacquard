use core::fmt;

use serde::Serialize;

use crate::types::{
    aturi::UriPath,
    nsid::Nsid,
    recordkey::{RecordKey, RecordKeyType, Rkey},
};

/// Trait for a collection of records that can be stored in a repository.
///
/// The records all have the same Lexicon schema.
///
/// Implemented on the record type itself.
pub trait Collection: fmt::Debug + Serialize {
    /// The NSID for the Lexicon that defines the schema of records in this collection.
    const NSID: &'static str;

    /// Returns the [`Nsid`] for the Lexicon that defines the schema of records in this
    /// collection.
    ///
    /// This is a convenience method that parses [`Self::NSID`].
    ///
    /// # Panics
    ///
    /// Panics if [`Self::NSID`] is not a valid NSID.
    ///
    /// [`Nsid`]: string::Nsid
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
    /// [`Nsid`]: string::Nsid
    fn repo_path<'u, T: RecordKeyType>(
        rkey: &'u crate::types::recordkey::RecordKey<T>,
    ) -> UriPath<'u> {
        UriPath {
            collection: Self::nsid(),
            rkey: Some(RecordKey::from(Rkey::raw(rkey.as_ref()))),
        }
    }
}
