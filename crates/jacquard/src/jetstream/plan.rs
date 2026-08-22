//! Replay planning: page `planSnapshot` over the sealed archive.
//!
//! The plan loop sets `sealedTipSeq` from the first response, sends it
//! as `beforeSeq` on subsequent pages, and pages with
//! `afterSeq = plannedThroughSeq` (exclusive) until it reaches the tip.
//!
//! Segments arrive with `mode: segment` (whole-file download)
//! or `mode: blocks` (per-block ranges) and a checksum

use jacquard_api::network_bsky::jetstream::plan_snapshot::{PlanSnapshot, PlanSnapshotOutput};
use jacquard_common::types::did::Did;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::{BosStr, DefaultStr};
use smol_str::{SmolStr, format_smolstr};

use super::archive::{JetstreamClient, JetstreamError, PlanSnapshotPage};
use jacquard_api::network_bsky::jetstream::plan_snapshot::PlanSnapshotKinds;

/// The event-kind filter vocabulary: the generated
/// [`SubscribeEventsKinds`] enum from the lexicon's `kinds` parameter.
pub use jacquard_api::network_bsky::jetstream::subscribe_events::SubscribeEventsKinds as EventKind;

/// A collection filter: an exact NSID or a namespace wildcard
/// (`app.bsky.feed.*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionFilter<S: BosStr = DefaultStr> {
    /// Matches exactly one collection.
    Exact(Nsid<S>),
    /// Matches every collection under the namespace prefix.
    /// (`app.bsky.feed.*` matches `app.bsky.feed.post`, for example.)
    Wildcard(S),
}

impl<S: BosStr + Clone> CollectionFilter<S> {
    /// Parse a filter expression: `nsid` or `prefix.*`.
    pub fn parse(s: S) -> Result<Self, FilterParseError> {
        if let Some(prefix) = s.clone().as_ref().strip_suffix(".*") {
            // Wildcard prefixes must themselves be valid NSID authority
            // prefixes (e.g. `app.bsky.feed`).
            Nsid::new(prefix)
                .map(|_| Self::Wildcard(S::from(s.clone())))
                .map_err(|_| FilterParseError {
                    reason: format_smolstr!(
                        "not a valid NSID or prefix.* wildcard: {}",
                        s.as_ref()
                    ),
                })
        } else {
            Nsid::new(s.clone())
                .map(Self::Exact)
                .map_err(|_| FilterParseError {
                    reason: format_smolstr!(
                        "not a valid NSID or prefix.* wildcard: {}",
                        s.as_ref()
                    ),
                })
        }
    }

    /// Whether a collection NSID matches: exact, or any collection under
    /// the wildcard's namespace (`app.bsky.feed.*` matches
    /// `app.bsky.feed.post` but not `app.bsky.feed2.post`).
    pub fn matches(&self, collection: &str) -> bool {
        match self {
            Self::Exact(nsid) => nsid.as_ref() == collection,
            Self::Wildcard(pattern) => {
                let pattern: &str = pattern.as_ref();
                // Safe: constructed from `prefix.*` with a valid prefix.
                let prefix = &pattern[..pattern.len() - 2];
                collection.starts_with(prefix)
                    && collection.as_bytes().get(prefix.len()) == Some(&b'.')
            }
        }
    }
}

/// Invalid filter expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterParseError {
    /// Human-readable reason.
    pub reason: SmolStr,
}

impl core::fmt::Display for FilterParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid replay filter: {}", self.reason)
    }
}

impl std::error::Error for FilterParseError {}

/// Filters applied to both the plan and every decoded row.
///
/// Server plans come from bloom filters and may over-include; the same
/// filters are re-checked client-side per decoded row. All syntax is
/// validated at construction so downstream request building is infallible.
#[derive(Debug, Clone, Default)]
pub struct ReplayFilters<S: BosStr = DefaultStr> {
    /// Event kinds to receive; empty = all.
    pub kinds: Vec<EventKind<S>>,
    /// Validated repo DIDs; empty = all.
    pub dids: Vec<Did<S>>,
    /// Exact-or-wildcard collection filters; empty = all.
    pub collections: Vec<CollectionFilter<S>>,
}

impl<S: BosStr + Clone> ReplayFilters<S> {
    /// Whether a decoded row passes the filters (exact client-side
    /// re-application; wildcards supported for collections).
    ///
    /// Per the lexicon, collection filters constrain commit events only;
    /// identity/account/sync rows carry no collection and pass them.
    pub fn matches(&self, kind: EventKind<&str>, did: &str, collection: &str) -> bool {
        if !self.kinds.is_empty() && !self.kinds.iter().any(|k| k.as_str() == kind.as_str()) {
            return false;
        }
        if !self.dids.is_empty() && !self.dids.iter().any(|d| d.as_ref() == did) {
            return false;
        }
        if kind.as_str() == "commit"
            && !self.collections.is_empty()
            && !self.collections.iter().any(|c| c.matches(collection))
        {
            return false;
        }
        true
    }
}

/// Errors from the plan loop.
#[derive(Debug)]
pub enum PlanError<E> {
    /// Transport-level failure (see [`ArchiveError`]).
    Archive(JetstreamError<E>),
    /// A page claimed no progress; paging would loop forever.
    NoProgress {
        /// The `plannedThroughSeq` the page returned without advancing.
        planned_through: i64,
    },
}

impl<E: core::fmt::Display> core::fmt::Display for PlanError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "{e}"),
            Self::NoProgress { planned_through } => write!(
                f,
                "planSnapshot page made no progress past {planned_through}"
            ),
        }
    }
}

impl<E: core::fmt::Display + core::fmt::Debug> std::error::Error for PlanError<E> {}

impl<E> From<JetstreamError<E>> for PlanError<E> {
    fn from(e: JetstreamError<E>) -> Self {
        Self::Archive(e)
    }
}

/// A `planSnapshot` paging cursor over the sealed range.
///
/// Pages from `after_seq` (exclusive; `None` plans the entire sealed
/// archive) until the sealed tip. `before` (the caller's snapshot bound,
/// inclusive) is sent on the first page and pinned to
/// `min(sealedTipSeq, before)` for later pages, so the planned range
/// cannot move past the requested bound.
///
/// Each [`PlanCursor::next_page`] call returns a page that owns its
/// response buffer; [`PlanSnapshotPage::parse`] then decodes into the
/// caller's chosen string backing, borrowed or owned — no backing is
/// forced anywhere in the paging path.
pub struct PlanCursor<'a> {
    client_filters: &'a ReplayFilters,
    after: Option<i64>,
    before: Option<i64>,
    sealed_tip: Option<i64>,
    done: bool,
}

impl<'a> PlanCursor<'a> {
    /// Start planning from `after_seq` (exclusive), bounded by `before`
    /// (inclusive) when set.
    pub fn new(filters: &'a ReplayFilters, after_seq: Option<i64>, before: Option<i64>) -> Self {
        Self {
            client_filters: filters,
            after: after_seq,
            before,
            sealed_tip: None,
            done: after_seq == Some(i64::MAX),
        }
    }

    /// The tip pinned by the first fetched page, once known.
    pub fn sealed_tip(&self) -> Option<i64> {
        self.sealed_tip
    }

    /// Fetch the next page, or `None` once the tip is reached.
    pub async fn next_page<C: jacquard_common::http_client::HttpClient>(
        &mut self,
        client: &JetstreamClient<C>,
    ) -> Result<Option<PlanSnapshotPage>, PlanError<C::Error>> {
        if self.done {
            return Ok(None);
        }
        let filters = self.client_filters;
        let params = PlanSnapshot::<DefaultStr> {
            after_seq: self.after,
            before_seq: if self.sealed_tip.is_none() {
                self.before
            } else {
                self.sealed_tip
            },
            collections: (!filters.collections.is_empty()).then(|| {
                filters
                    .collections
                    .iter()
                    .map(|c| match c {
                        CollectionFilter::Exact(nsid) => SmolStr::from(nsid.as_ref()),
                        CollectionFilter::Wildcard(pattern) => pattern.clone(),
                    })
                    .collect::<Vec<_>>()
            }),
            dids: (!filters.dids.is_empty()).then(|| filters.dids.clone()),
            kinds: (!filters.kinds.is_empty()).then(|| {
                filters
                    .kinds
                    .iter()
                    .map(|k| PlanSnapshotKinds::from_value(DefaultStr::from(k.as_str())))
                    .collect()
            }),
            extra_data: None,
        };
        let page = client.plan_snapshot_page(&params).await?;
        // Inspect the envelope with a borrowed parse; the returned page
        // keeps the buffer for the caller's own choice of backing.
        let envelope: PlanSnapshotOutput<&str> = page
            .parse()
            .map_err(|e| PlanError::Archive(JetstreamError::Decode(e.0)))?;

        let tip = self.sealed_tip.unwrap_or_else(|| {
            self.before
                .map_or(envelope.sealed_tip_seq, |b| envelope.sealed_tip_seq.min(b))
        });
        self.sealed_tip = Some(tip);
        if envelope.planned_through_seq >= tip {
            self.done = true;
        } else if envelope.planned_through_seq <= self.after.unwrap_or(0) {
            return Err(PlanError::NoProgress {
                planned_through: envelope.planned_through_seq,
            });
        } else {
            self.after = Some(envelope.planned_through_seq);
        }
        Ok(Some(page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_filter_matches_namespace_boundary_only() {
        let filter = CollectionFilter::parse("app.bsky.feed.*").expect("valid wildcard");
        assert!(filter.matches("app.bsky.feed.post"));
        assert!(!filter.matches("app.bsky.feed2.post"));
        assert!(!filter.matches("app.bsky.feedpost"));
    }

    #[test]
    fn collection_filters_do_not_reject_non_commit_rows() {
        let filters = ReplayFilters {
            kinds: Vec::new(),
            dids: Vec::new(),
            collections: vec![CollectionFilter::parse("app.bsky.feed.post").expect("valid nsid")],
        };
        // Identity rows carry an empty collection; the lexicon scopes
        // collection filters to commit events only.
        assert!(filters.matches(EventKind::Identity, "did:plc:x", ""));
        assert!(filters.matches(EventKind::Sync, "did:plc:x", ""));
        assert!(!filters.matches(EventKind::Commit, "did:plc:x", "app.bsky.graph.follow"));
        assert!(filters.matches(EventKind::Commit, "did:plc:x", "app.bsky.feed.post"));
    }
}
