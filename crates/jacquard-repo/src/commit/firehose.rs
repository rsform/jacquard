//! Firehose commit message structures
//!
//! These structures are vendored from `jacquard-api::com_atproto::sync::subscribe_repos`
//! to avoid a dependency on the full API crate. They represent firehose protocol messages,
//! which are DISTINCT from repository commit objects.

use bytes::Bytes;
use jacquard_common::types::cid::CidLink;
use jacquard_common::types::crypto::PublicKey;
use jacquard_common::types::string::{Datetime, Did, Tid};
use jacquard_common::{CowStr, IntoStatic};

/// Firehose commit message (sync v1.0 and v1.1)
///
/// Represents an update of repository state in the firehose stream.
/// This is the message format sent over `com.atproto.sync.subscribeRepos`.
///
/// **Sync v1.0 vs v1.1:**
/// - v1.0: `prev_data` is None/skipped, consumers must have sufficient previous repository state to validate
/// - v1.1: `prev_data` includes previous MST root for inductive validation
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirehoseCommit<'a> {
    /// The repo this event comes from
    #[serde(borrow)]
    pub repo: Did<'a>,

    /// The rev of the emitted commit
    pub rev: Tid,

    /// The stream sequence number of this message
    pub seq: i64,

    /// The rev of the last emitted commit from this repo (if any)
    pub since: Tid,

    /// Timestamp of when this message was originally broadcast
    pub time: Datetime,

    /// Repo commit object CID
    ///
    /// This CID points to the repository commit block (with did, version, data, rev, prev, sig).
    /// It must be the first entry in the CAR header 'roots' list.
    #[serde(borrow)]
    pub commit: CidLink<'a>,

    /// CAR file containing relevant blocks
    ///
    /// Contains blocks as a diff since the previous repo state. The commit block
    /// must be included, and its CID must be the first root in the CAR header.
    ///
    /// For sync v1.1, may include additional MST node blocks needed for operation inversion.
    #[serde(with = "super::serde_bytes_helper")]
    pub blocks: Bytes,

    /// Operations in this commit
    #[serde(borrow)]
    pub ops: Vec<RepoOp<'a>>,

    /// Previous MST root CID (sync v1.1 only)
    ///
    /// The root CID of the MST tree for the previous commit (indicated by the 'since' field).
    /// Corresponds to the 'data' field in the previous repo commit object.
    ///
    /// **Sync v1.1 inductive validation:**
    /// - Enables validation without local MST state
    /// - Operations can be inverted (creates→deletes, deletes→creates with prev values)
    /// - Required for "inductive firehose" consumption
    ///
    /// **Sync v1.0:**
    /// - This field is None
    /// - Consumers must have previous repository state
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub prev_data: Option<CidLink<'a>>,

    /// Blob CIDs referenced in this commit
    #[serde(borrow)]
    pub blobs: Vec<CidLink<'a>>,

    /// DEPRECATED: Replaced by #sync event and data limits
    ///
    /// Indicates that this commit contained too many ops, or data size was too large.
    /// Consumers will need to make a separate request to get missing data.
    pub too_big: bool,

    /// DEPRECATED: Unused
    pub rebase: bool,
}

/// A repository operation (mutation of a single record)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoOp<'a> {
    /// Operation type: "create", "update", or "delete"
    #[serde(borrow)]
    pub action: CowStr<'a>,

    /// Collection/rkey path (e.g., "app.bsky.feed.post/abc123")
    #[serde(borrow)]
    pub path: CowStr<'a>,

    /// For creates and updates, the new record CID. For deletions, None (null).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cid: Option<CidLink<'a>>,

    /// For updates and deletes, the previous record CID
    ///
    /// Required for sync v1.1 inductive firehose validation.
    /// For creates, this field should not be defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub prev: Option<CidLink<'a>>,
}

impl IntoStatic for FirehoseCommit<'_> {
    type Output = FirehoseCommit<'static>;

    fn into_static(self) -> Self::Output {
        FirehoseCommit {
            repo: self.repo.into_static(),
            rev: self.rev,
            seq: self.seq,
            since: self.since,
            time: self.time,
            commit: self.commit.into_static(),
            blocks: self.blocks,
            ops: self.ops.into_iter().map(|op| op.into_static()).collect(),
            prev_data: self.prev_data.map(|pd| pd.into_static()),
            blobs: self.blobs.into_iter().map(|b| b.into_static()).collect(),
            too_big: self.too_big,
            rebase: self.rebase,
        }
    }
}

impl IntoStatic for RepoOp<'_> {
    type Output = RepoOp<'static>;

    fn into_static(self) -> Self::Output {
        RepoOp {
            action: self.action.into_static(),
            path: self.path.into_static(),
            cid: self.cid.into_static(),
            prev: self.prev.map(|p| p.into_static()),
        }
    }
}

use crate::car::parse_car_bytes;
/// Validation functions for firehose commit messages
///
/// These functions validate commits from the `com.atproto.sync.subscribeRepos` firehose.
use crate::error::{RepoError, Result};
use crate::mst::Mst;
use crate::storage::{BlockStore, LayeredBlockStore, MemoryBlockStore};
use cid::Cid as IpldCid;
use std::sync::Arc;

impl<'a> FirehoseCommit<'a> {
    /// Validate a sync v1.0 commit
    ///
    /// **Requirements:**
    /// - Must have previous MST state (potentially full repository)
    /// - All blocks needed for validation must be in `self.blocks`
    ///
    /// **Validation steps:**
    /// 1. Parse CAR blocks from `self.blocks` into temporary storage
    /// 2. Load commit object and verify signature
    /// 3. Apply operations to previous MST (using temporary storage for new blocks)
    /// 4. Verify result matches commit.data (new MST root)
    ///
    /// Returns the new MST root CID on success.
    pub async fn validate_v1_0<S: BlockStore + Sync + 'static>(
        &self,
        prev_mst_root: Option<IpldCid>,
        prev_storage: Arc<S>,
        pubkey: &PublicKey<'_>,
    ) -> Result<IpldCid> {
        // 1. Parse CAR blocks from the firehose message into temporary storage
        let parsed = parse_car_bytes(&self.blocks).await?;
        let temp_storage = MemoryBlockStore::new_from_blocks(parsed.blocks);

        // 2. Create layered storage: reads from temp first, then prev; writes to temp only
        // This avoids copying all previous MST blocks
        let layered_storage = LayeredBlockStore::new(temp_storage.clone(), prev_storage);

        // 3. Extract and verify commit object from temporary storage
        let commit_cid: IpldCid = self
            .commit
            .to_ipld()
            .map_err(|e| RepoError::invalid_cid_conversion(e, "commit CID"))?;
        let commit_bytes = temp_storage
            .get(&commit_cid)
            .await?
            .ok_or_else(|| RepoError::not_found("commit block", &commit_cid))?;

        let commit = super::Commit::from_cbor(&commit_bytes)?;

        // Verify DID matches
        if commit.did().as_ref() != self.repo.as_ref() {
            return Err(RepoError::invalid_commit(format!(
                "DID mismatch: commit has {}, message has {}",
                commit.did(),
                self.repo
            ))
            .with_help("DID mismatch indicates the commit was signed by a different identity - verify the commit is from the expected repository"));
        }

        // Verify signature
        commit.verify(pubkey)?;

        let layered_arc = Arc::new(layered_storage);

        // 4. Load previous MST state from layered storage (or start empty)
        let prev_mst = if let Some(prev_root) = prev_mst_root {
            Mst::load(layered_arc.clone(), prev_root, None)
        } else {
            Mst::new(layered_arc.clone())
        };

        // 5. Load new MST from commit.data (claimed result)
        let expected_root = *commit.data();
        let new_mst = Mst::load(layered_arc, expected_root, None);

        // 6. Compute diff to get verified write ops (with actual prev values from tree state)
        let diff = prev_mst.diff(&new_mst).await?;
        let verified_ops = diff.to_verified_ops();

        // 7. Apply verified ops to prev MST
        let computed_mst = prev_mst.batch(&verified_ops).await?;

        // 8. Verify computed result matches claimed result
        let computed_root = computed_mst.get_pointer().await?;

        if computed_root != expected_root {
            return Err(RepoError::cid_mismatch(format!(
                "MST root mismatch: expected {}, got {}",
                expected_root, computed_root
            )));
        }

        Ok(expected_root)
    }

    /// Validate a sync v1.1 commit (inductive validation)
    ///
    /// **Requirements:**
    /// - `self.prev_data` must be Some (contains previous MST root)
    /// - All blocks needed for validation must be in `self.blocks`
    ///
    /// **Validation steps:**
    /// 1. Parse CAR blocks from `self.blocks` into temporary storage
    /// 2. Load commit object and verify signature
    /// 3. Start from `prev_data` MST root (loaded from temp storage)
    /// 4. Apply operations (with prev CID validation for updates/deletes)
    /// 5. Verify result matches commit.data (new MST root)
    ///
    /// Returns the new MST root CID on success.
    ///
    /// **Inductive property:** Can validate without any external state besides the blocks
    /// in this message. The `prev_data` field provides the starting MST root, and operations
    /// include `prev` CIDs for validation. All necessary blocks must be in the CAR bytes.
    pub async fn validate_v1_1(&self, pubkey: &PublicKey<'_>) -> Result<IpldCid> {
        // 1. Require prev_data for v1.1
        let prev_data_cid: IpldCid = self
            .prev_data
            .as_ref()
            .ok_or_else(|| {
                RepoError::invalid_commit("Sync v1.1 validation requires prev_data field")
            })?
            .to_ipld()
            .map_err(|e| RepoError::invalid_cid_conversion(e, "prev_data CID"))?;

        // 2. Parse CAR blocks from the firehose message into temporary storage
        let parsed = parse_car_bytes(&self.blocks).await?;
        let temp_storage = Arc::new(MemoryBlockStore::new_from_blocks(parsed.blocks));

        // 3. Extract and verify commit object from temporary storage
        let commit_cid: IpldCid = self
            .commit
            .to_ipld()
            .map_err(|e| RepoError::invalid_cid_conversion(e, "commit CID"))?;
        let commit_bytes = temp_storage
            .get(&commit_cid)
            .await?
            .ok_or_else(|| RepoError::not_found("commit block", &commit_cid))?;

        let commit = super::Commit::from_cbor(&commit_bytes)?;

        // Verify DID matches
        if commit.did().as_ref() != self.repo.as_ref() {
            return Err(RepoError::invalid_commit(format!(
                "DID mismatch: commit has {}, message has {}",
                commit.did(),
                self.repo
            ))
            .with_help("DID mismatch indicates the commit was signed by a different identity - verify the commit is from the expected repository"));
        }

        // Verify signature
        commit.verify(pubkey)?;

        // 4. Load previous MST from prev_data (all blocks should be in temp_storage)
        let prev_mst = Mst::load(temp_storage.clone(), prev_data_cid, None);

        // 5. Load new MST from commit.data (claimed result)
        let expected_root = *commit.data();
        let new_mst = Mst::load(temp_storage, expected_root, None);

        // 6. Compute diff to get verified write ops (with actual prev values from tree state)
        let diff = prev_mst.diff(&new_mst).await?;
        let verified_ops = diff.to_verified_ops();

        // 7. Apply verified ops to prev MST
        let computed_mst = prev_mst.batch(&verified_ops).await?;

        // 8. Verify computed result matches claimed result
        let computed_root = computed_mst.get_pointer().await?;

        if computed_root != expected_root {
            return Err(RepoError::cid_mismatch(format!(
                "MST root mismatch: expected {}, got {}",
                expected_root, computed_root
            )));
        }

        Ok(expected_root)
    }
}
