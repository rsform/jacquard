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
use smol_str::ToSmolStr;

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

impl<'a> RepoOp<'a> {
    /// Convert to VerifiedWriteOp for v1.1 validation
    ///
    /// Validates that all required fields are present for inversion.
    pub fn to_invertible_op(&self) -> Result<VerifiedWriteOp> {
        let key = self.path.to_smolstr();

        match self.action.as_ref() {
            "create" => {
                let cid = self
                    .cid
                    .as_ref()
                    .ok_or_else(|| RepoError::invalid_commit("create operation missing cid field"))?
                    .to_ipld()
                    .map_err(|e| RepoError::invalid_cid_conversion(e, "create cid"))?;

                Ok(VerifiedWriteOp::Create { key, cid })
            }
            "update" => {
                let cid = self
                    .cid
                    .as_ref()
                    .ok_or_else(|| RepoError::invalid_commit("update operation missing cid field"))?
                    .to_ipld()
                    .map_err(|e| RepoError::invalid_cid_conversion(e, "update cid"))?;

                let prev = self
                    .prev
                    .as_ref()
                    .ok_or_else(|| {
                        RepoError::invalid_commit(
                            "update operation missing prev field for v1.1 validation",
                        )
                    })?
                    .to_ipld()
                    .map_err(|e| RepoError::invalid_cid_conversion(e, "update prev"))?;

                Ok(VerifiedWriteOp::Update { key, cid, prev })
            }
            "delete" => {
                let prev = self
                    .prev
                    .as_ref()
                    .ok_or_else(|| {
                        RepoError::invalid_commit(
                            "delete operation missing prev field for v1.1 validation",
                        )
                    })?
                    .to_ipld()
                    .map_err(|e| RepoError::invalid_cid_conversion(e, "delete prev"))?;

                Ok(VerifiedWriteOp::Delete { key, prev })
            }
            action => Err(RepoError::invalid_commit(format!(
                "unknown action type: {}",
                action
            ))),
        }
    }
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
use crate::mst::{Mst, VerifiedWriteOp};
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
    ///
    /// Note: Because this uses the same merkle search tree struct as the repository itself,
    /// this is far from the most efficient possible validation function possible. The repo
    /// tree struct carries extra information. However,
    /// it has the virtue of making everything self-validating.
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

        // 5. Load new MST from commit.data (claimed result)
        let expected_root = *commit.data();

        let mut new_mst = Mst::load(temp_storage, expected_root, None);

        let verified_ops = self
            .ops
            .iter()
            .filter_map(|op| op.to_invertible_op().ok())
            .collect::<Vec<_>>();
        if verified_ops.len() != self.ops.len() {
            return Err(RepoError::invalid_commit(format!(
                "Invalid commit: expected {} ops, got {}",
                self.ops.len(),
                verified_ops.len()
            )));
        }

        for op in verified_ops {
            if let Ok(inverted) = new_mst.invert_op(op.clone()).await {
                if !inverted {
                    return Err(RepoError::invalid_commit(format!(
                        "Invalid commit: op {:?} is not invertible",
                        op
                    )));
                }
            }
        }
        // 8. Verify computed previous state matches claimed previous state
        let computed_root = new_mst.get_pointer().await?;

        if computed_root != prev_data_cid {
            return Err(RepoError::cid_mismatch(format!(
                "MST root mismatch: expected {}, got {}",
                prev_data_cid, computed_root
            )));
        }

        Ok(expected_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Repository;
    use crate::commit::Commit;
    use crate::mst::{Mst, RecordWriteOp};
    use crate::storage::MemoryBlockStore;
    use jacquard_common::types::crypto::{KeyCodec, PublicKey};
    use jacquard_common::types::recordkey::Rkey;
    use jacquard_common::types::string::{Nsid, RecordKey};
    use jacquard_common::types::tid::Ticker;
    use jacquard_common::types::value::RawData;
    use smol_str::SmolStr;
    use std::collections::BTreeMap;

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
        let data = mst.persist().await.unwrap();

        let rev = Ticker::new().next(None);
        let commit = Commit::new_unsigned(did.into_static(), data, rev, None)
            .sign(&signing_key)
            .unwrap();

        let commit_cbor = commit.to_cbor().unwrap();
        let commit_cid = storage.put(&commit_cbor).await.unwrap();

        Repository::new(storage, mst, commit.into_static(), commit_cid)
    }

    fn get_public_key(signing_key: &k256::ecdsa::SigningKey) -> PublicKey<'static> {
        let verifying_key = signing_key.verifying_key();
        let pubkey_bytes = verifying_key.to_encoded_point(true).as_bytes().to_vec();
        PublicKey {
            codec: KeyCodec::Secp256k1,
            bytes: pubkey_bytes.into(),
        }
    }

    #[tokio::test]
    async fn test_valid_v1_1_commit_roundtrip() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("test1").unwrap());

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        // Create operation
        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: rkey.clone(),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        // Convert to firehose commit (v1.1 includes prev_data)
        let firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        // Validate using v1.1 validation
        let result = firehose_commit.validate_v1_1(&pubkey).await;
        if let Err(ref e) = result {
            eprintln!("Validation error: {}", e);
        }
        assert!(result.is_ok(), "Valid v1.1 commit should pass validation");

        let validated_root = result.unwrap();
        assert_eq!(
            validated_root, commit_data.data,
            "Validated root should match commit data root"
        );
    }

    #[tokio::test]
    async fn test_valid_v1_0_commit_with_prev_storage() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let rkey = RecordKey(Rkey::new("test1").unwrap());

        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        let prev_root = *repo.current_commit().data();

        // Create operation
        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: rkey.clone(),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        // For v1.0, we strip prev_data
        let mut firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        firehose_commit.prev_data = None;

        // Validate using v1.0 validation with previous storage
        let result = firehose_commit
            .validate_v1_0(Some(prev_root), storage.clone(), &pubkey)
            .await;

        assert!(result.is_ok(), "Valid v1.0 commit should pass validation");

        let validated_root = result.unwrap();
        assert_eq!(
            validated_root, commit_data.data,
            "Validated root should match commit data root"
        );
    }

    #[tokio::test]
    async fn test_multiple_operations_roundtrip() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        // First commit: create two records
        let ops1 = vec![
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: RecordKey(Rkey::new("post1").unwrap()),
                record: make_test_record(1),
            },
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: RecordKey(Rkey::new("post2").unwrap()),
                record: make_test_record(2),
            },
        ];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops1,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(result.is_ok(), "Multiple creates should validate");
    }

    #[tokio::test]
    async fn test_update_and_delete_operations_roundtrip() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        // First: create records
        let rkey1 = RecordKey(Rkey::new("post1").unwrap());
        let rkey2 = RecordKey(Rkey::new("post2").unwrap());

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

        let (_, commit_data) = repo
            .create_commit(
                &create_ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        repo.apply_commit(commit_data).await.unwrap();

        // Second: update one, delete the other
        let update_ops = vec![
            RecordWriteOp::Update {
                collection: collection.clone(),
                rkey: rkey1.clone(),
                record: make_test_record(10),
                prev: None,
            },
            RecordWriteOp::Delete {
                collection: collection.clone(),
                rkey: rkey2.clone(),
                prev: None,
            },
        ];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &update_ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let firehose_commit = commit_data
            .to_firehose_commit(&did, 2, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_ok(),
            "Update and delete operations should validate"
        );
    }

    #[tokio::test]
    async fn test_missing_commit_block_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: RecordKey(Rkey::new("test1").unwrap()),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let mut firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        // Parse CAR and remove commit block
        let parsed = parse_car_bytes(&firehose_commit.blocks).await.unwrap();
        let commit_cid: IpldCid = firehose_commit.commit.to_ipld().unwrap();

        let blocks_without_commit: BTreeMap<IpldCid, bytes::Bytes> = parsed
            .blocks
            .into_iter()
            .filter(|(cid, _)| cid != &commit_cid)
            .collect();

        // Rebuild CAR without commit block
        let bad_car = crate::car::write_car_bytes(commit_cid, blocks_without_commit)
            .await
            .unwrap();

        firehose_commit.blocks = bad_car.into();

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_err(),
            "Validation should fail when commit block is missing"
        );
    }

    #[tokio::test]
    async fn test_missing_mst_blocks_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        // Create multiple records to ensure MST has nodes
        let ops = vec![
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: RecordKey(Rkey::new("aaa").unwrap()),
                record: make_test_record(1),
            },
            RecordWriteOp::Create {
                collection: collection.clone(),
                rkey: RecordKey(Rkey::new("zzz").unwrap()),
                record: make_test_record(2),
            },
        ];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let mut firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        // Parse CAR and keep only commit block (remove MST nodes)
        let parsed = parse_car_bytes(&firehose_commit.blocks).await.unwrap();
        let commit_cid: IpldCid = firehose_commit.commit.to_ipld().unwrap();

        let blocks_commit_only: BTreeMap<IpldCid, bytes::Bytes> = parsed
            .blocks
            .into_iter()
            .filter(|(cid, _)| cid == &commit_cid)
            .collect();

        let bad_car = crate::car::write_car_bytes(commit_cid, blocks_commit_only)
            .await
            .unwrap();

        firehose_commit.blocks = bad_car.into();

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_err(),
            "Validation should fail when MST blocks are missing"
        );
    }

    #[tokio::test]
    async fn test_wrong_mst_root_in_commit_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: RecordKey(Rkey::new("test1").unwrap()),
            record: make_test_record(1),
        }];

        let (repo_ops, mut commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        // Create a fake commit with wrong data root
        use crate::mst::util::compute_cid;
        let wrong_root = compute_cid(&[1, 2, 3, 4]).unwrap();

        let fake_commit = Commit::new_unsigned(
            did.clone().into_static(),
            wrong_root,
            commit_data.rev.clone(),
            commit_data.prev,
        )
        .sign(&signing_key)
        .unwrap();

        let fake_commit_cbor = fake_commit.to_cbor().unwrap();
        let fake_commit_cid = compute_cid(&fake_commit_cbor).unwrap();

        // Replace commit block in blocks
        commit_data.blocks.remove(&commit_data.cid);
        commit_data
            .blocks
            .insert(fake_commit_cid, bytes::Bytes::from(fake_commit_cbor));
        commit_data.cid = fake_commit_cid;

        let firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_err(),
            "Validation should fail when commit has wrong MST root"
        );
    }

    #[tokio::test]
    async fn test_mismatched_did_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let wrong_did = Did::new("did:plc:wrong").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: RecordKey(Rkey::new("test1").unwrap()),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        // Create firehose commit with wrong DID
        let mut firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        firehose_commit.repo = wrong_did;

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_err(),
            "Validation should fail with mismatched DID"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("DID mismatch"),
            "Error should mention DID mismatch"
        );
    }

    #[tokio::test]
    async fn test_invalid_signature_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

        // Use a different key for verification
        let wrong_signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let wrong_pubkey = get_public_key(&wrong_signing_key);

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: RecordKey(Rkey::new("test1").unwrap()),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        let result = firehose_commit.validate_v1_1(&wrong_pubkey).await;
        assert!(
            result.is_err(),
            "Validation should fail with wrong public key"
        );
    }

    #[tokio::test]
    async fn test_missing_prev_data_for_v1_1_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: RecordKey(Rkey::new("test1").unwrap()),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let mut firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        // Strip prev_data to make it invalid for v1.1
        firehose_commit.prev_data = None;

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_err(),
            "v1.1 validation should fail without prev_data"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("prev_data"),
            "Error should mention missing prev_data"
        );
    }

    #[tokio::test]
    async fn test_wrong_prev_data_cid_fails() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mut repo = create_test_repo(storage.clone()).await;

        let collection = Nsid::new("app.bsky.feed.post").unwrap();
        let did = Did::new("did:plc:test").unwrap();
        let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let pubkey = get_public_key(&signing_key);

        let ops = vec![RecordWriteOp::Create {
            collection: collection.clone(),
            rkey: RecordKey(Rkey::new("test1").unwrap()),
            record: make_test_record(1),
        }];

        let (repo_ops, commit_data) = repo
            .create_commit(
                &ops,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        let mut firehose_commit = commit_data
            .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        // Use wrong prev_data CID (point to commit instead of MST root)
        firehose_commit.prev_data = Some(firehose_commit.commit.clone());

        let result = firehose_commit.validate_v1_1(&pubkey).await;
        assert!(
            result.is_err(),
            "Validation should fail with wrong prev_data CID"
        );
    }
}
