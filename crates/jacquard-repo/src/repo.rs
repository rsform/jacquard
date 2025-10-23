//! High-level repository operations
//!
//! Optional convenience layer over MST primitives. Provides type-safe record operations,
//! batch writes, commit creation, and CAR export.

use crate::commit::firehose::{FirehoseCommit, RepoOp};
use crate::commit::{Commit, SigningKey};
use crate::error::{RepoError, Result};
use crate::mst::{Mst, RecordWriteOp};
use crate::storage::BlockStore;
use bytes::Bytes;
use cid::Cid as IpldCid;
use jacquard_common::IntoStatic;
use jacquard_common::types::cid::CidLink;
use jacquard_common::types::recordkey::RecordKeyType;
use jacquard_common::types::string::{Datetime, Did, Nsid, RecordKey, Tid};
use jacquard_common::types::tid::Ticker;
use smol_str::format_smolstr;
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::sync::Arc;

/// Commit data for repository updates
///
/// Contains signed commit and all blocks needed for persistence.
/// Follows the rsky pattern of separating commit formatting from application.
#[derive(Debug, Clone)]
pub struct CommitData {
    /// Commit CID
    pub cid: IpldCid,

    /// New revision TID
    pub rev: Tid,

    /// Previous revision TID (None for initial commit)
    pub since: Option<Tid>,

    /// Previous commit CID (None for initial commit)
    pub prev: Option<IpldCid>,

    /// New MST root CID
    pub data: IpldCid,

    /// Previous MST root CID (for sync v1.1)
    pub prev_data: Option<IpldCid>,

    /// New blocks to persist (MST nodes + record data + commit block)
    pub blocks: BTreeMap<IpldCid, bytes::Bytes>,

    /// Relevant blocks for firehose (sync v1.1 inductive validation)
    ///
    /// Subset of `blocks` containing:
    /// - Commit block
    /// - MST node blocks along paths for all changed keys
    /// - Includes "adjacent" blocks needed for operation inversion
    pub relevant_blocks: BTreeMap<IpldCid, bytes::Bytes>,

    /// CIDs of blocks to delete
    pub deleted_cids: Vec<IpldCid>,
}

impl CommitData {
    /// Generate a firehose commit message (sync v1.1)
    ///
    /// Converts this commit into a `FirehoseCommit` with `prev_data` field
    /// and relevant blocks for inductive validation.
    pub async fn to_firehose_commit(
        &self,
        repo: &Did<'_>,
        seq: i64,
        time: Datetime,
        ops: Vec<RepoOp<'static>>,
        blobs: Vec<CidLink<'static>>,
    ) -> Result<FirehoseCommit<'static>> {
        let mut proof_blocks = self.blocks.clone();
        proof_blocks.append(&mut self.relevant_blocks.clone());
        // Convert relevant blocks to CAR format
        let blocks_car = crate::car::write_car_bytes(self.cid, proof_blocks).await?;

        Ok(FirehoseCommit {
            repo: repo.clone().into_static(),
            rev: self.rev.clone(),
            seq,
            since: self.since.clone().unwrap_or_else(|| self.rev.clone()),
            time,
            commit: CidLink::from(self.cid),
            blocks: blocks_car.into(),
            ops,
            prev_data: self.prev_data.map(CidLink::from),
            blobs,
            too_big: false,
            rebase: false,
        })
    }
}

/// High-level repository operations
///
/// Provides a convenient API over MST primitives for common repository workflows.
///
/// # Example
///
/// ```rust,ignore
/// use jacquard_repo::{Repository, MemoryBlockStore};
/// use jacquard_common::types::string::{Did, Nsid, RecordKey};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let storage = Arc::new(MemoryBlockStore::new());
/// let mut repo = create_test_repo(storage).await;
///
/// let collection = Nsid::new("app.bsky.feed.post")?;
/// let rkey = RecordKey::new("3l5yhcgz7y42y")?;
/// let record_cid = /* ... compute CID of record ... */;
///
/// repo.create_record(&collection, &rkey, record_cid).await?;
///
/// let did = Did::new("did:plc:example")?;
/// let signing_key = /* ... load key ... */;
/// let commit_cid = repo.commit(&did, None, &signing_key).await?;
///
/// repo.export_car("repo.car", commit_cid).await?;
/// # Ok(())
/// # }
/// ```
pub struct Repository<S: BlockStore> {
    mst: Mst<S>,
    storage: Arc<S>,
    commit: Commit<'static>,
    commit_cid: IpldCid,
}

impl<S: BlockStore + Sync + 'static> Repository<S> {
    /// Create repository from existing components
    ///
    /// Static constructor for when you already have the MST, commit, and CID.
    pub fn new(storage: Arc<S>, mst: Mst<S>, commit: Commit<'static>, commit_cid: IpldCid) -> Self {
        Self {
            storage,
            mst,
            commit,
            commit_cid,
        }
    }

    /// Load repository from commit CID
    pub async fn from_commit(storage: Arc<S>, commit_cid: &IpldCid) -> Result<Self> {
        let commit_bytes = storage
            .get(commit_cid)
            .await?
            .ok_or_else(|| {
                RepoError::not_found("commit", commit_cid)
                    .with_help("Commit must be applied to storage before loading repository - use apply_commit() or ensure commit is persisted")
            })?;

        let commit = Commit::from_cbor(&commit_bytes)?;
        let mst_root = commit.data();

        let mst = Mst::load(storage.clone(), *mst_root, None);

        Ok(Self {
            mst,
            storage,
            commit: commit.into_static(),
            commit_cid: *commit_cid,
        })
    }

    /// Format an initial commit for a new repository
    ///
    /// Creates an empty MST, optionally applies initial record writes, signs the commit,
    /// and returns CommitData ready to apply to storage.
    ///
    /// This does NOT persist to storage - use `create_from_commit` or `create` for that.
    pub async fn format_init_commit<K>(
        storage: Arc<S>,
        did: Did<'static>,
        signing_key: &K,
        initial_writes: Option<&[RecordWriteOp<'_>]>,
    ) -> Result<CommitData>
    where
        K: SigningKey,
    {
        let mut mst = Mst::new(storage.clone());
        let mut blocks = BTreeMap::new();

        // Apply initial writes if provided
        if let Some(ops) = initial_writes {
            for op in ops {
                let key = format_smolstr!("{}/{}", op.collection().as_ref(), op.rkey().as_ref());

                match op {
                    RecordWriteOp::Create { record, .. } => {
                        // Serialize and store record
                        let cbor = serde_ipld_dagcbor::to_vec(record)
                            .map_err(|e| RepoError::serialization(e))?;
                        let cid = storage.put(&cbor).await?;
                        blocks.insert(cid, bytes::Bytes::from(cbor));

                        mst = mst.add(key.as_str(), cid).await?;
                    }
                    RecordWriteOp::Update { .. } | RecordWriteOp::Delete { .. } => {
                        return Err(RepoError::invalid_commit(
                            "Initial commit can only contain creates",
                        ));
                    }
                }
            }
        }

        // Persist MST and collect blocks
        let data = mst.persist().await?;
        let diff = Mst::new(storage.clone()).diff(&mst).await?;
        blocks.extend(diff.new_mst_blocks);

        // Create and sign initial commit
        let rev = Ticker::new().next(None);
        let commit = Commit::new_unsigned(did, data, rev.clone(), None).sign(signing_key)?;

        let commit_cbor = commit.to_cbor()?;
        let commit_cid = crate::mst::util::compute_cid(&commit_cbor)?;
        let commit_bytes = bytes::Bytes::from(commit_cbor);

        blocks.insert(commit_cid, commit_bytes.clone());

        Ok(CommitData {
            cid: commit_cid,
            rev,
            since: None,
            prev: None,
            data,
            prev_data: None,
            blocks: blocks.clone(),
            relevant_blocks: blocks,
            deleted_cids: Vec::new(),
        })
    }

    /// Create repository from CommitData
    ///
    /// Applies the commit to storage and loads the repository from it.
    pub async fn create_from_commit(storage: Arc<S>, commit_data: CommitData) -> Result<Self> {
        let commit_cid = commit_data.cid;
        storage.apply_commit(commit_data).await?;
        Self::from_commit(storage, &commit_cid).await
    }

    /// Create a new repository
    ///
    /// Convenience method that formats an initial commit and applies it to storage.
    pub async fn create<K>(
        storage: Arc<S>,
        did: Did<'static>,
        signing_key: &K,
        initial_writes: Option<&[RecordWriteOp<'_>]>,
    ) -> Result<Self>
    where
        K: SigningKey,
    {
        let commit =
            Self::format_init_commit(storage.clone(), did, signing_key, initial_writes).await?;
        Self::create_from_commit(storage, commit).await
    }

    /// Get a record by collection and rkey
    pub async fn get_record<T: RecordKeyType>(
        &self,
        collection: &Nsid<'_>,
        rkey: &RecordKey<T>,
    ) -> Result<Option<IpldCid>> {
        let key = format!("{}/{}", collection.as_ref(), rkey.as_ref());
        self.mst.get(&key).await
    }

    /// Create a record (error if exists)
    pub async fn create_record<T: RecordKeyType>(
        &mut self,
        collection: &Nsid<'_>,
        rkey: &RecordKey<T>,
        record_cid: IpldCid,
    ) -> Result<()> {
        let key = format!("{}/{}", collection.as_ref(), rkey.as_ref());

        if self.mst.get(&key).await?.is_some() {
            return Err(RepoError::already_exists("record", &key));
        }

        self.mst = self.mst.add(&key, record_cid).await?;
        Ok(())
    }

    /// Update a record (error if not exists, returns previous CID)
    pub async fn update_record<T: RecordKeyType>(
        &mut self,
        collection: &Nsid<'_>,
        rkey: &RecordKey<T>,
        record_cid: IpldCid,
    ) -> Result<IpldCid> {
        let key = format!("{}/{}", collection.as_ref(), rkey.as_ref());

        let old_cid = self
            .mst
            .get(&key)
            .await?
            .ok_or_else(|| RepoError::not_found("record", &key))?;

        self.mst = self.mst.update(&key, record_cid).await?;
        Ok(old_cid)
    }

    /// Delete a record (error if not exists, returns deleted CID)
    pub async fn delete_record<T: RecordKeyType>(
        &mut self,
        collection: &Nsid<'_>,
        rkey: &RecordKey<T>,
    ) -> Result<IpldCid> {
        let key = format!("{}/{}", collection.as_ref(), rkey.as_ref());

        let old_cid = self
            .mst
            .get(&key)
            .await?
            .ok_or_else(|| RepoError::not_found("record", &key))?;

        self.mst = self.mst.delete(&key).await?;
        Ok(old_cid)
    }

    // TODO(cursor-based queries): Potential future API additions
    //
    // The current API is purely single-record CRUD. Cursor-based traversal (see mst/cursor.rs)
    // would enable efficient collection/range queries:
    //
    // - list_collection(collection: &Nsid) -> Vec<(RecordKey, IpldCid)>
    //   Enumerate all records in a collection via prefix search on "collection/"
    //   Uses cursor.advance() + cursor.skip_subtree() to skip irrelevant branches
    //
    // - list_collection_range(collection: &Nsid, start: &Rkey, end: &Rkey) -> Vec<...>
    //   Range query: advance to start key, collect until > end, skip subtrees outside range
    //   Useful for pagination / time-bounded queries (since Rkeys are often TIDs)
    //
    // - list_all_collections() -> Vec<Nsid>
    //   Walk tree, track collection prefixes, skip subtrees once prefix changes
    //
    // Current single-key get() is already optimal (O(log n) targeted lookup).
    // But these bulk operations would benefit significantly from cursor's skip_subtree()
    // to avoid traversing unrelated branches when searching lexicographically-organized data.

    /// Create a commit from record write operations
    ///
    /// Applies write operations, creates signed commit, and collects blocks:
    /// - Serializes records to DAG-CBOR and stores data blocks
    /// - Applies operations to MST and computes diff
    /// - Walks paths to include blocks required for sync v1.1 inductive proof
    /// - Opts to overprovide blocks slightly so that a proof consistently succeeds
    /// - Empirical heuristic rather than mathematical assurance, but it works
    ///
    /// Returns `(ops, CommitData)` - ops are needed for `to_firehose_commit()`.
    pub async fn create_commit<K>(
        &mut self,
        ops: &[RecordWriteOp<'_>],
        did: &Did<'_>,
        prev: Option<IpldCid>,
        signing_key: &K,
    ) -> Result<(Vec<RepoOp<'static>>, CommitData)>
    where
        K: SigningKey,
    {
        // Step 1: Apply all write operations to build new MST and collect leaf blocks
        let mut updated_tree = self.mst.clone();
        let mut leaf_blocks = BTreeMap::new();

        for op in ops {
            updated_tree = match op {
                RecordWriteOp::Create {
                    collection,
                    rkey,
                    record,
                } => {
                    let key = format_smolstr!("{}/{}", collection.as_ref(), rkey.as_ref());

                    // Serialize record to DAG-CBOR
                    let cbor = serde_ipld_dagcbor::to_vec(record).map_err(|e| {
                        RepoError::serialization(e).with_context(format!(
                            "serializing record data for {}/{}",
                            collection.as_ref(),
                            rkey.as_ref()
                        ))
                    })?;

                    // Compute CID and store data
                    let cid = self.storage.put(&cbor).await?;
                    leaf_blocks.insert(cid.clone(), Bytes::from(cbor));

                    updated_tree.add(key.as_str(), cid).await?
                }
                RecordWriteOp::Update {
                    collection,
                    rkey,
                    record,
                    prev,
                } => {
                    let key = format_smolstr!("{}/{}", collection.as_ref(), rkey.as_ref());

                    // Serialize record to DAG-CBOR
                    let cbor = serde_ipld_dagcbor::to_vec(record).map_err(|e| {
                        RepoError::serialization(e).with_context(format!(
                            "serializing record data for {}/{}",
                            collection.as_ref(),
                            rkey.as_ref()
                        ))
                    })?;

                    // Compute CID and store data
                    let cid = self.storage.put(&cbor).await?;

                    // Validate prev if provided
                    if let Some(prev_cid) = prev {
                        if &cid != prev_cid {
                            return Err(RepoError::cid_mismatch(format!(
                                "Update prev CID mismatch for key {}: expected {}, got {}",
                                key, prev_cid, cid
                            )));
                        }
                    }

                    leaf_blocks.insert(cid.clone(), Bytes::from(cbor));

                    updated_tree.add(key.as_str(), cid).await?
                }
                RecordWriteOp::Delete {
                    collection,
                    rkey,
                    prev,
                } => {
                    let key = format_smolstr!("{}/{}", collection.as_ref(), rkey.as_ref());

                    // Validate prev if provided
                    if let Some(prev_cid) = prev {
                        // Check exists
                        let current = self
                            .mst
                            .get(key.as_str())
                            .await?
                            .ok_or_else(|| RepoError::not_found("record", key.as_str()))?;
                        if &current != prev_cid {
                            return Err(RepoError::cid_mismatch(format!(
                                "Delete prev CID mismatch for key {}: expected {}, got {}",
                                key, prev_cid, current
                            )));
                        }
                    }

                    updated_tree.delete(key.as_str()).await?
                }
            };
        }

        // Step 2: Compute diff and get new MST root
        let data = updated_tree.root().await?;
        let prev_data = *self.commit.data();
        let diff = self.mst.diff(&updated_tree).await?;

        // Step 3: Extract everything we need from diff
        let repo_ops = diff
            .to_repo_ops()
            .into_iter()
            .map(|op| op.into_static())
            .collect();

        // Step 4: Build blocks and relevant_blocks collections using diff tracking
        //
        // CRITICAL: This logic is validated against 16384 test cases in tests/mst_diff_suite.rs
        // Any changes here MUST pass that test (zero missing blocks required for inductive validation)
        //
        // Inductive validation requirements (sync v1.1):
        // - Include MST nodes along operation paths in BOTH old and new trees
        // - Filter out deleted MST blocks (they're in removed_mst_blocks)
        // - Include all new record data (leaf_blocks)
        let mut blocks = diff.new_mst_blocks;
        blocks.extend(leaf_blocks.clone()); // Include record data blocks
        let mut relevant_blocks = BTreeMap::new();
        relevant_blocks.extend(leaf_blocks); // Include record data in relevant blocks too

        for op in ops {
            let key = format_smolstr!("{}/{}", op.collection().as_ref(), op.rkey().as_ref());
            // New tree path (inclusion proof for creates/updates, exclusion for deletes)
            updated_tree
                .blocks_for_path(&key, &mut relevant_blocks)
                .await?;

            // Old tree path (needed for inductive validation)
            // - CREATE: exclusion proof (key didn't exist)
            // - UPDATE: show what changed
            // - DELETE: show what was deleted
            self.mst.blocks_for_path(&key, &mut relevant_blocks).await?;
        }

        // Filter out deleted blocks before combining
        let removed_set: std::collections::HashSet<_> =
            diff.removed_mst_blocks.iter().copied().collect();
        relevant_blocks.retain(|cid, _| !removed_set.contains(cid));

        let deleted_cids = diff.removed_cids;

        // Step 5: Create and sign commit
        let rev = Ticker::new().next(Some(self.commit.rev.clone()));
        let commit = Commit::new_unsigned(did.clone().into_static(), data, rev.clone(), prev)
            .sign(signing_key)?;

        let commit_cbor = commit.to_cbor()?;
        let commit_cid = crate::mst::util::compute_cid(&commit_cbor)?;
        let commit_bytes = bytes::Bytes::from(commit_cbor);

        // Step 6: Add commit block to both collections
        blocks.insert(commit_cid, commit_bytes.clone());
        relevant_blocks.insert(commit_cid, commit_bytes);

        // Step 7: Update internal MST state
        self.mst = updated_tree;

        Ok((
            repo_ops,
            CommitData {
                cid: commit_cid,
                rev,
                since: Some(self.commit.rev.clone()),
                prev,
                data,
                prev_data: Some(prev_data),
                blocks,
                relevant_blocks,
                deleted_cids,
            },
        ))
    }

    /// Apply a commit (persist blocks to storage)
    ///
    /// Persists all blocks from `CommitData` and updates internal state.
    /// Uses `BlockStore::apply_commit()` to perform atomic write+delete operations.
    pub async fn apply_commit(&mut self, commit_data: CommitData) -> Result<IpldCid> {
        let commit_cid = commit_data.cid;

        // Apply commit to storage (writes new blocks, deletes garbage)
        self.storage.apply_commit(commit_data).await?;

        // Load and update internal state
        let commit_bytes = self
            .storage
            .get(&commit_cid)
            .await?
            .ok_or_else(|| {
                RepoError::not_found("commit block", &commit_cid)
                    .with_help("Commit block should have been persisted by apply_commit() - this indicates a storage inconsistency")
            })?;
        let commit = Commit::from_cbor(&commit_bytes)?;

        self.commit = commit.into_static();
        self.commit_cid = commit_cid;

        // Reload MST from new root
        self.mst = Mst::load(self.storage.clone(), *self.commit.data(), None);

        Ok(commit_cid)
    }

    /// Create a commit for the current repository state
    ///
    /// Convenience method that calls `create_commit()` with no additional operations
    /// and `apply_commit()`. Use this after manually updating the MST with individual
    /// record operations (e.g., `create_record()`, `update_record()`, `delete_record()`).
    pub async fn commit<K>(
        &mut self,
        did: &Did<'_>,
        prev: Option<IpldCid>,
        signing_key: &K,
    ) -> Result<(Vec<RepoOp<'static>>, IpldCid)>
    where
        K: SigningKey,
    {
        let (ops, commit_data) = self.create_commit(&[], did, prev, signing_key).await?;
        Ok((ops, self.apply_commit(commit_data).await?))
    }

    /// Export repository to CAR file
    pub async fn export_car(&self, path: impl AsRef<Path>, commit_cid: IpldCid) -> Result<()> {
        crate::car::export_repo_car(path, commit_cid, &self.mst).await
    }

    /// Get the underlying MST
    pub fn mst(&self) -> &Mst<S> {
        &self.mst
    }

    /// Get reference to the storage
    pub fn storage(&self) -> &Arc<S> {
        &self.storage
    }

    /// Get the current commit
    pub fn current_commit(&self) -> &Commit<'static> {
        &self.commit
    }

    /// Get the current commit CID
    pub fn current_commit_cid(&self) -> &IpldCid {
        &self.commit_cid
    }

    /// Get the DID from the current commit
    pub fn did(&self) -> &Did<'_> {
        self.commit.did()
    }
}

impl<S: BlockStore> Display for Repository<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use crate::mst::tree::short_cid;

        writeln!(f, "Repository {{")?;
        writeln!(f, "  DID: {}", self.commit.did())?;
        writeln!(f, "  Commit: {}", short_cid(&self.commit_cid))?;
        writeln!(f, "  Rev: {}", self.commit.rev)?;
        writeln!(f, "  Data: {}", short_cid(self.commit.data()))?;
        writeln!(f, "  MST:")?;

        // Format MST with indentation
        let mst_display = format!("{}", self.mst);
        for line in mst_display.lines() {
            writeln!(f, "    {}", line)?;
        }

        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, str::FromStr};

    use super::*;
    use crate::storage::MemoryBlockStore;
    use jacquard_common::types::{
        crypto::{KeyCodec, PublicKey},
        recordkey::Rkey,
        value::RawData,
    };
    use smol_str::SmolStr;

    fn make_test_cid(value: u8) -> IpldCid {
        use crate::DAG_CBOR_CID_CODEC;
        use jacquard_common::types::crypto::SHA2_256;
        use sha2::{Digest, Sha256};

        let hash = Sha256::digest(&[value]);
        let mh = multihash::Multihash::wrap(SHA2_256, &hash).unwrap();
        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    fn make_test_record(n: u32) -> BTreeMap<SmolStr, RawData<'static>> {
        let mut record = BTreeMap::new();
        record.insert(
            SmolStr::new("$type"),
            RawData::String("app.bsky.feed.post".into()),
        );
        record.insert(
            SmolStr::new("text"),
            RawData::String(format!("Test post #{}", n).into()),
        );
        record.insert(
            SmolStr::new("createdAt"),
            RawData::String("2024-01-01T00:00:00Z".to_string().into()),
        );
        record
    }

    async fn create_test_repo(storage: Arc<MemoryBlockStore>) -> Repository<MemoryBlockStore> {
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

        let mst = Mst::new(storage.clone());
        let data = mst.persist().await.unwrap(); // Persist empty MST

        let rev = Ticker::new().next(None);
        let commit = Commit::new_unsigned(did.into_static(), data, rev, None)
            .sign(&signing_key)
            .unwrap();

        let commit_cbor = commit.to_cbor().unwrap();
        let commit_cid = storage.put(&commit_cbor).await.unwrap();

        Repository::new(storage, mst, commit.into_static(), commit_cid)
    }

    #[tokio::test]
    async fn test_create_and_get_record() {
        use crate::mst::RecordWriteOp;

        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone().into_static(),
            rkey: rkey.clone(),
            record: make_test_record(1),
        }];

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        assert_eq!(repo_ops.len(), 1);
        assert_eq!(repo_ops[0].action.as_ref(), "create");

        let retrieved = repo.get_record(&collection, &rkey).await.unwrap();
        assert!(retrieved.is_some());

        // Verify data is actually in storage (from commit_data blocks)
        let cid = retrieved.unwrap();
        assert!(commit_data.blocks.contains_key(&cid));
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid = make_test_cid(1);

        repo.create_record(&collection, &rkey, cid).await.unwrap();

        let result = repo
            .create_record(&collection, &rkey, make_test_cid(2))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_record() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);

        repo.create_record(&collection, &rkey, cid1).await.unwrap();

        let old = repo.update_record(&collection, &rkey, cid2).await.unwrap();
        assert_eq!(old, cid1);

        let retrieved = repo.get_record(&collection, &rkey).await.unwrap();
        assert_eq!(retrieved, Some(cid2));
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid = make_test_cid(1);

        let result = repo.update_record(&collection, &rkey, cid).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_record() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid = make_test_cid(1);

        repo.create_record(&collection, &rkey, cid).await.unwrap();

        let deleted = repo.delete_record(&collection, &rkey).await.unwrap();
        assert_eq!(deleted, cid);

        let retrieved = repo.get_record(&collection, &rkey).await.unwrap();
        assert_eq!(retrieved, None);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());

        let result = repo.delete_record(&collection, &rkey).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_from_commit() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid = make_test_cid(1);

        repo.create_record(&collection, &rkey, cid).await.unwrap();

        // Persist MST
        repo.mst.persist().await.unwrap();

        // Create commit (need a signing key for this test)
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let (_, commit_cid) = repo.commit(&did, None, &signing_key).await.unwrap();

        // Load from commit
        let loaded_repo = Repository::from_commit(storage, &commit_cid).await.unwrap();

        let retrieved = loaded_repo.get_record(&collection, &rkey).await.unwrap();
        assert_eq!(retrieved, Some(cid));
    }

    #[tokio::test]
    async fn test_commit_creates_valid_commit() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid = make_test_cid(1);

        repo.create_record(&collection, &rkey, cid).await.unwrap();
        repo.mst.persist().await.unwrap();

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let (_, commit_cid) = repo.commit(&did, None, &signing_key).await.unwrap();

        // Verify commit was stored
        let commit_bytes = storage.get(&commit_cid).await.unwrap();
        assert!(commit_bytes.is_some());

        // Verify commit can be deserialized
        let bytes = commit_bytes.unwrap();
        let commit = Commit::from_cbor(&bytes).unwrap();
        assert_eq!(commit.did().as_ref(), did.as_ref());
        let root_cid = repo.mst.root().await.unwrap();
        assert_eq!(commit.data(), &root_cid);
    }

    #[tokio::test]
    async fn test_sequential_operations() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("test1").unwrap());
        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);

        // Create
        repo.create_record(&collection, &rkey, cid1).await.unwrap();
        let got = repo.get_record(&collection, &rkey).await.unwrap();
        assert_eq!(got, Some(cid1));

        // Update
        let old = repo.update_record(&collection, &rkey, cid2).await.unwrap();
        assert_eq!(old, cid1);
        let got = repo.get_record(&collection, &rkey).await.unwrap();
        assert_eq!(got, Some(cid2));

        // Delete
        let deleted = repo.delete_record(&collection, &rkey).await.unwrap();
        assert_eq!(deleted, cid2);
        let got = repo.get_record(&collection, &rkey).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn test_large_scale_operations() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let mut ticker = Ticker::new();

        // Add 100 records
        let mut records = Vec::new();
        for i in 0..100 {
            let tid_str = ticker.next(None).into_static();
            let rkey = RecordKey(Rkey::from_str(tid_str.as_str()).unwrap());
            let cid = make_test_cid((i % 256) as u8);
            repo.create_record(&collection, &rkey, cid).await.unwrap();
            records.push((rkey, cid));
        }

        // Verify all records exist
        for (rkey, cid) in &records {
            let got = repo.get_record(&collection, rkey).await.unwrap();
            assert_eq!(got, Some(*cid));
        }

        // Update first 20 records
        for i in 0..20 {
            let (rkey, _old_cid) = &records[i];
            let new_cid = make_test_cid(((i + 100) % 256) as u8);
            repo.update_record(&collection, rkey, new_cid)
                .await
                .unwrap();

            let got = repo.get_record(&collection, rkey).await.unwrap();
            assert_eq!(got, Some(new_cid));
        }

        // Delete last 20 records
        for i in 80..100 {
            let (rkey, cid) = &records[i];
            let deleted = repo.delete_record(&collection, rkey).await.unwrap();
            assert_eq!(deleted, *cid);

            let got = repo.get_record(&collection, rkey).await.unwrap();
            assert!(got.is_none());
        }
    }

    #[tokio::test]
    async fn test_commit_signature_verification() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("abc123").unwrap());
        let cid = make_test_cid(1);

        repo.create_record(&collection, &rkey, cid).await.unwrap();
        repo.mst.persist().await.unwrap();

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

        // Get public key from signing key
        let verifying_key = signing_key.verifying_key();
        let pubkey_bytes = verifying_key.to_encoded_point(true).as_bytes().to_vec();
        let pubkey = PublicKey {
            codec: KeyCodec::Secp256k1,
            bytes: pubkey_bytes.into(),
        };

        let (_, commit_cid) = repo.commit(&did, None, &signing_key).await.unwrap();

        // Load commit and verify signature
        let commit_bytes = storage.get(&commit_cid).await.unwrap().unwrap();
        let commit = Commit::from_cbor(&commit_bytes).unwrap();

        // Signature verification should succeed
        commit.verify(&pubkey).unwrap();
    }

    #[tokio::test]
    async fn test_load_from_storage_with_multiple_commits() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

        // Add some records and commit
        let mut records = Vec::new();
        for i in 0..10 {
            let rkey = RecordKey(Rkey::from_str(&format!("record{}", i)).unwrap());
            let cid = make_test_cid(i as u8);
            repo.create_record(&collection, &rkey, cid).await.unwrap();
            records.push((rkey, cid));
        }
        repo.mst.persist().await.unwrap();
        let (_, commit_cid) = repo
            .commit(&did, Some(repo.current_commit_cid().clone()), &signing_key)
            .await
            .unwrap();

        // Load repository from storage
        let loaded_repo = Repository::from_commit(storage.clone(), &commit_cid)
            .await
            .unwrap();

        // Verify all records are accessible
        for (rkey, cid) in &records {
            let got = loaded_repo.get_record(&collection, rkey).await.unwrap();
            assert_eq!(got, Some(*cid));
        }

        // Verify metadata matches
        assert_eq!(loaded_repo.did().as_ref(), did.as_ref());
        assert_eq!(loaded_repo.current_commit().version, 3);
        assert_eq!(loaded_repo.current_commit_cid(), &commit_cid);
    }

    #[tokio::test]
    async fn test_commit_tracks_deleted_cids() {
        use crate::mst::RecordWriteOp;

        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey1 = RecordKey(Rkey::new("test1").unwrap());
        let rkey2 = RecordKey(Rkey::new("test2").unwrap());

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

        // Create records with actual data
        let create_ops = vec![
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey1.clone(),
                record: make_test_record(1),
            },
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey2.clone(),
                record: make_test_record(2),
            },
        ];

        let (_repo_ops, commit_data) = repo
            .create_commit(
                &create_ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let cid1 = repo.get_record(&collection, &rkey1).await.unwrap().unwrap();

        repo.apply_commit(commit_data).await.unwrap();

        // Delete one record and format commit (don't apply yet)
        let delete_ops = vec![RecordWriteOp::Delete {
            collection: collection.clone(),
            rkey: rkey1.clone(),
            prev: None,
        }];

        let (_, commit_data) = repo
            .create_commit(
                &delete_ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        // Verify deleted_cids contains the deleted record CID
        assert_eq!(commit_data.deleted_cids.len(), 1);
        assert_eq!(commit_data.deleted_cids[0], cid1);
    }

    #[tokio::test]
    async fn test_record_writes_with_commit_includes_data_blocks() {
        use crate::mst::RecordWriteOp;

        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey1 = RecordKey(Rkey::new("post1").unwrap());
        let rkey2 = RecordKey(Rkey::new("post2").unwrap());

        // Create records with actual data
        let ops = vec![
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey1.clone(),
                record: make_test_record(1),
            },
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey2.clone(),
                record: make_test_record(2),
            },
        ];

        // Format commit
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let cid1 = repo.get_record(&collection, &rkey1).await.unwrap().unwrap();
        let cid2 = repo.get_record(&collection, &rkey2).await.unwrap().unwrap();

        // Verify commit data includes record data blocks
        assert!(
            commit_data.blocks.contains_key(&cid1),
            "blocks should contain record 1 data"
        );
        assert!(
            commit_data.blocks.contains_key(&cid2),
            "blocks should contain record 2 data"
        );
        assert!(
            commit_data.relevant_blocks.contains_key(&cid1),
            "relevant_blocks should contain record 1 data"
        );
        assert!(
            commit_data.relevant_blocks.contains_key(&cid2),
            "relevant_blocks should contain record 2 data"
        );

        // Verify we can deserialize the record data
        let record1_bytes = commit_data.blocks.get(&cid1).unwrap();
        let record1: BTreeMap<SmolStr, RawData> =
            serde_ipld_dagcbor::from_slice(record1_bytes).unwrap();
        assert_eq!(
            record1.get(&SmolStr::new("text")).unwrap(),
            &jacquard_common::types::value::RawData::String("Test post #1".to_string().into())
        );

        // Verify firehose ops
        assert_eq!(repo_ops.len(), 2);
        assert_eq!(repo_ops[0].action.as_ref(), "create");
        assert_eq!(repo_ops[1].action.as_ref(), "create");
    }

    #[tokio::test]
    async fn test_batch_mixed_operations() {
        use crate::mst::RecordWriteOp;

        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();

        // Pre-populate with some records
        let rkey1 = RecordKey(Rkey::new("existing1").unwrap());
        let rkey2 = RecordKey(Rkey::new("existing2").unwrap());
        let rkey3 = RecordKey(Rkey::new("existing3").unwrap());

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

        let create_ops = vec![
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey1.clone(),
                record: make_test_record(1),
            },
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey2.clone(),
                record: make_test_record(2),
            },
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: rkey3.clone(),
                record: make_test_record(3),
            },
        ];

        let (_, commit_data) = repo
            .create_commit(
                &create_ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        // Get the CID of existing1 so we can verify it changed
        let old_cid1 = repo.get_record(&collection, &rkey1).await.unwrap().unwrap();

        repo.apply_commit(commit_data).await.unwrap();

        // Batch operation: create new, update existing, delete existing
        let new_rkey = RecordKey(Rkey::new("new1").unwrap());
        let ops = vec![
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: new_rkey.clone(),
                record: make_test_record(10),
            },
            RecordWriteOp::Update {
                collection: collection.clone(),
                rkey: rkey1.clone(),
                record: make_test_record(11),
                prev: None,
            },
            RecordWriteOp::Delete {
                collection: collection.clone(),
                rkey: rkey2.clone(),
                prev: None,
            },
        ];

        let (repo_ops, _commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        assert_eq!(repo_ops.len(), 3);

        // Verify final state
        let new_cid = repo.get_record(&collection, &new_rkey).await.unwrap();
        assert!(new_cid.is_some(), "new record should exist");

        let updated_cid1 = repo.get_record(&collection, &rkey1).await.unwrap();
        assert!(updated_cid1.is_some(), "updated record should exist");
        assert_ne!(
            updated_cid1.unwrap(),
            old_cid1,
            "record should have new CID"
        );

        assert_eq!(repo.get_record(&collection, &rkey2).await.unwrap(), None);
        assert!(
            repo.get_record(&collection, &rkey3)
                .await
                .unwrap()
                .is_some()
        );
    }
}
