//! MST diff calculation

use super::tree::Mst;
use crate::error::Result;
use crate::storage::BlockStore;
use cid::Cid as IpldCid;
use smol_str::SmolStr;
use std::collections::HashMap;

/// Diff between two MST states
///
/// Represents the changes needed to transform one tree into another.
/// Used for firehose validation and batch operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MstDiff {
    /// New records created (key, new CID)
    pub creates: Vec<(SmolStr, IpldCid)>,

    /// Records updated (key, new CID, old CID)
    pub updates: Vec<(SmolStr, IpldCid, IpldCid)>,

    /// Records deleted (key, old CID)
    pub deletes: Vec<(SmolStr, IpldCid)>,
}

use super::tree::VerifiedWriteOp;

impl MstDiff {
    /// Create empty diff
    pub fn new() -> Self {
        Self {
            creates: Vec::new(),
            updates: Vec::new(),
            deletes: Vec::new(),
        }
    }

    /// Check if diff is empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.creates.is_empty() && self.updates.is_empty() && self.deletes.is_empty()
    }

    /// Count total operations
    pub fn op_count(&self) -> usize {
        self.creates.len() + self.updates.len() + self.deletes.len()
    }

    /// Validate against sync v1.1 limits
    ///
    /// The sync protocol has a 200 operation limit per commit.
    pub fn validate_limits(&self) -> Result<()> {
        if self.op_count() > 200 {
            return Err(crate::error::RepoError::too_large(
                "diff operation count",
                self.op_count(),
                200,
            ));
        }
        Ok(())
    }

    /// Convert diff to verified write operations
    ///
    /// Returns operations that can be safely applied with `batch()`.
    /// All update/delete operations include verified prev CIDs.
    pub fn to_verified_ops(&self) -> Vec<VerifiedWriteOp> {
        let mut ops = Vec::with_capacity(self.op_count());

        // Add creates
        for (key, cid) in &self.creates {
            ops.push(VerifiedWriteOp::Create {
                key: key.clone(),
                cid: *cid,
            });
        }

        // Add updates (includes prev)
        for (key, new_cid, old_cid) in &self.updates {
            ops.push(VerifiedWriteOp::Update {
                key: key.clone(),
                cid: *new_cid,
                prev: *old_cid,
            });
        }

        // Add deletes (includes prev)
        for (key, old_cid) in &self.deletes {
            ops.push(VerifiedWriteOp::Delete {
                key: key.clone(),
                prev: *old_cid,
            });
        }

        ops
    }

    /// Convert diff to firehose repository operations
    ///
    /// Returns operations in the format used by `com.atproto.sync.subscribeRepos`.
    /// All update/delete operations include prev CIDs for sync v1.1 validation.
    pub fn to_repo_ops(&self) -> Vec<crate::commit::firehose::RepoOp<'_>> {
        use jacquard_common::types::cid::CidLink;

        let mut ops = Vec::with_capacity(self.op_count());

        // Add creates
        for (key, cid) in &self.creates {
            ops.push(crate::commit::firehose::RepoOp {
                action: "create".into(),
                path: key.as_str().into(),
                cid: Some(CidLink::from(*cid)),
                prev: None,
            });
        }

        // Add updates
        for (key, new_cid, old_cid) in &self.updates {
            ops.push(crate::commit::firehose::RepoOp {
                action: "update".into(),
                path: key.as_str().into(),
                cid: Some(CidLink::from(*new_cid)),
                prev: Some(CidLink::from(*old_cid)),
            });
        }

        // Add deletes
        for (key, old_cid) in &self.deletes {
            ops.push(crate::commit::firehose::RepoOp {
                action: "delete".into(),
                path: key.as_str().into(),
                cid: None, // null for deletes
                prev: Some(CidLink::from(*old_cid)),
            });
        }

        ops
    }
}

impl Default for MstDiff {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: BlockStore + Sync + 'static> Mst<S> {
    /// Compute diff from this tree to another
    ///
    /// Returns operations needed to transform `self` into `other`.
    /// - Creates: keys in `other` but not in `self`
    /// - Updates: keys in both but with different CIDs
    /// - Deletes: keys in `self` but not in `other`
    pub async fn diff(&self, other: &Mst<S>) -> Result<MstDiff> {
        // Collect all leaves from both trees
        let self_leaves = self.leaves().await?;
        let other_leaves = other.leaves().await?;

        // Build hashmaps for efficient lookup
        let self_map: HashMap<SmolStr, IpldCid> = self_leaves.into_iter().collect();
        let other_map: HashMap<SmolStr, IpldCid> = other_leaves.into_iter().collect();

        let mut diff = MstDiff::new();

        // Find creates and updates
        for (key, new_cid) in &other_map {
            match self_map.get(key) {
                Some(old_cid) => {
                    // Key exists in both - check if CID changed
                    if old_cid != new_cid {
                        diff.updates.push((key.clone(), *new_cid, *old_cid));
                    }
                }
                None => {
                    // Key only in other - create
                    diff.creates.push((key.clone(), *new_cid));
                }
            }
        }

        // Find deletes
        for (key, old_cid) in &self_map {
            if !other_map.contains_key(key) {
                // Key only in self - delete
                diff.deletes.push((key.clone(), *old_cid));
            }
        }

        Ok(diff)
    }

    /// Compute diff from this tree to empty (all deletes)
    ///
    /// Returns diff representing deletion of all records in this tree.
    pub async fn diff_to_empty(&self) -> Result<MstDiff> {
        let leaves = self.leaves().await?;

        Ok(MstDiff {
            creates: Vec::new(),
            updates: Vec::new(),
            deletes: leaves,
        })
    }
}

#[cfg(test)]
mod tests {
    use jacquard_common::types::crypto::SHA2_256;

    use super::*;
    use crate::{DAG_CBOR_CID_CODEC, storage::memory::MemoryBlockStore};
    use std::sync::Arc;

    fn test_cid(n: u8) -> IpldCid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    #[tokio::test]
    async fn test_diff_empty_trees() {
        let storage = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage.clone());
        let tree2 = Mst::new(storage);

        let diff = tree1.diff(&tree2).await.unwrap();

        assert!(diff.is_empty());
        assert_eq!(diff.op_count(), 0);
    }

    #[tokio::test]
    async fn test_diff_creates() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("a", test_cid(1)).await.unwrap();
        let tree2 = tree2.add("b", test_cid(2)).await.unwrap();

        let diff = tree1.diff(&tree2).await.unwrap();

        assert_eq!(diff.creates.len(), 2);
        assert_eq!(diff.updates.len(), 0);
        assert_eq!(diff.deletes.len(), 0);
        assert_eq!(diff.op_count(), 2);

        // Check creates content
        assert!(
            diff.creates
                .iter()
                .any(|(k, c)| k == "a" && *c == test_cid(1))
        );
        assert!(
            diff.creates
                .iter()
                .any(|(k, c)| k == "b" && *c == test_cid(2))
        );
    }

    #[tokio::test]
    async fn test_diff_deletes() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("b", test_cid(2)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);

        let diff = tree1.diff(&tree2).await.unwrap();

        assert_eq!(diff.creates.len(), 0);
        assert_eq!(diff.updates.len(), 0);
        assert_eq!(diff.deletes.len(), 2);
        assert_eq!(diff.op_count(), 2);

        // Check deletes content
        assert!(
            diff.deletes
                .iter()
                .any(|(k, c)| k == "a" && *c == test_cid(1))
        );
        assert!(
            diff.deletes
                .iter()
                .any(|(k, c)| k == "b" && *c == test_cid(2))
        );
    }

    #[tokio::test]
    async fn test_diff_updates() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("b", test_cid(2)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("a", test_cid(10)).await.unwrap(); // Changed CID
        let tree2 = tree2.add("b", test_cid(2)).await.unwrap(); // Same CID

        let diff = tree1.diff(&tree2).await.unwrap();

        assert_eq!(diff.creates.len(), 0);
        assert_eq!(diff.updates.len(), 1); // Only "a" changed
        assert_eq!(diff.deletes.len(), 0);
        assert_eq!(diff.op_count(), 1);

        // Check update content
        assert_eq!(diff.updates[0].0, "a");
        assert_eq!(diff.updates[0].1, test_cid(10)); // new CID
        assert_eq!(diff.updates[0].2, test_cid(1)); // old CID
    }

    #[tokio::test]
    async fn test_diff_mixed_operations() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("b", test_cid(2)).await.unwrap();
        let tree1 = tree1.add("c", test_cid(3)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("a", test_cid(10)).await.unwrap(); // Updated
        let tree2 = tree2.add("b", test_cid(2)).await.unwrap(); // Unchanged
        // "c" deleted
        let tree2 = tree2.add("d", test_cid(4)).await.unwrap(); // Created

        let diff = tree1.diff(&tree2).await.unwrap();

        assert_eq!(diff.creates.len(), 1); // "d"
        assert_eq!(diff.updates.len(), 1); // "a"
        assert_eq!(diff.deletes.len(), 1); // "c"
        assert_eq!(diff.op_count(), 3);
    }

    #[tokio::test]
    async fn test_diff_to_empty() {
        let storage = Arc::new(MemoryBlockStore::new());
        let tree = Mst::new(storage);
        let tree = tree.add("a", test_cid(1)).await.unwrap();
        let tree = tree.add("b", test_cid(2)).await.unwrap();
        let tree = tree.add("c", test_cid(3)).await.unwrap();

        let diff = tree.diff_to_empty().await.unwrap();

        assert_eq!(diff.creates.len(), 0);
        assert_eq!(diff.updates.len(), 0);
        assert_eq!(diff.deletes.len(), 3);
        assert_eq!(diff.op_count(), 3);
    }

    #[tokio::test]
    async fn test_validate_limits() {
        let mut diff = MstDiff::new();

        // Add 200 creates (at limit)
        for i in 0..200 {
            diff.creates
                .push((SmolStr::new(&format!("key{}", i)), test_cid(1)));
        }

        // Should be ok at exactly 200
        assert!(diff.validate_limits().is_ok());

        // Add one more - should fail
        diff.creates.push((SmolStr::new("key201"), test_cid(1)));
        assert!(diff.validate_limits().is_err());
    }

    #[tokio::test]
    async fn test_diff_symmetry() {
        // diff(A, B) should be inverse of diff(B, A)
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("b", test_cid(2)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("b", test_cid(2)).await.unwrap();
        let tree2 = tree2.add("c", test_cid(3)).await.unwrap();

        let diff1 = tree1.diff(&tree2).await.unwrap();
        let diff2 = tree2.diff(&tree1).await.unwrap();

        // diff1: creates="c", deletes="a"
        // diff2: creates="a", deletes="c"
        assert_eq!(diff1.creates.len(), 1);
        assert_eq!(diff1.deletes.len(), 1);
        assert_eq!(diff2.creates.len(), 1);
        assert_eq!(diff2.deletes.len(), 1);

        assert_eq!(diff1.creates[0].0, diff2.deletes[0].0); // "c"
        assert_eq!(diff1.deletes[0].0, diff2.creates[0].0); // "a"
    }
}
