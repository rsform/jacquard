//! MST diff calculation

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use super::cursor::{CursorPosition, MstCursor};
use super::tree::Mst;
use super::util::serialize_node_data;
use crate::commit::firehose::RepoOp;
use crate::error::{RepoError, Result};
use crate::mst::NodeEntry;
use crate::storage::BlockStore;
use bytes::Bytes;
use cid::Cid as IpldCid;
use jacquard_common::types::cid::CidLink;
use smol_str::SmolStr;

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

    /// Record CIDs that are newly referenced (from creates + updates)
    ///
    /// This includes:
    /// - CIDs from created records
    /// - New CIDs from updated records
    ///
    /// These need to be available in storage for the new tree.
    pub new_leaf_cids: Vec<IpldCid>,

    /// Record CIDs that are no longer referenced (from deletes + updates)
    ///
    /// This includes:
    /// - CIDs from deleted records
    /// - Old CIDs from updated records
    ///
    /// These can be garbage collected if not referenced elsewhere.
    pub removed_cids: Vec<IpldCid>,

    /// MST node blocks that are newly created
    ///
    /// When modifying a tree, new MST nodes are created along changed paths.
    /// This tracks those nodes for persistence/commit inclusion.
    pub new_mst_blocks: BTreeMap<IpldCid, Bytes>,

    /// MST node blocks that are no longer needed
    ///
    /// When modifying a tree, old MST nodes along changed paths become unreachable.
    /// This tracks those nodes for garbage collection.
    pub removed_mst_blocks: Vec<IpldCid>,
}

use super::tree::VerifiedWriteOp;

impl MstDiff {
    /// Create empty diff
    pub fn new() -> Self {
        Self {
            creates: Vec::new(),
            updates: Vec::new(),
            deletes: Vec::new(),
            new_leaf_cids: Vec::new(),
            removed_cids: Vec::new(),
            new_mst_blocks: BTreeMap::new(),
            removed_mst_blocks: Vec::new(),
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
            return Err(RepoError::too_large(
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

    /// Fetch new record data blocks from storage
    ///
    /// Returns a map of CID → bytes for all new record data (creates + updates).
    /// This is useful for including record data in commits and firehose messages.
    pub async fn fetch_new_blocks<S: BlockStore>(
        &self,
        storage: &S,
    ) -> Result<std::collections::BTreeMap<IpldCid, bytes::Bytes>> {
        let mut blocks = BTreeMap::new();

        for cid in &self.new_leaf_cids {
            if let Some(block) = storage.get(cid).await? {
                blocks.insert(*cid, block);
            }
        }

        Ok(blocks)
    }

    /// Convert diff to firehose repository operations
    ///
    /// Returns operations in the format used by `com.atproto.sync.subscribeRepos`.
    /// All update/delete operations include prev CIDs for sync v1.1 validation.
    pub fn to_repo_ops(&self) -> Vec<RepoOp<'_>> {
        let mut ops = Vec::with_capacity(self.op_count());

        // Add creates
        for (key, cid) in &self.creates {
            ops.push(RepoOp {
                action: "create".into(),
                path: key.as_str().into(),
                cid: Some(CidLink::from(*cid)),
                prev: None,
            });
        }

        // Add updates
        for (key, new_cid, old_cid) in &self.updates {
            ops.push(RepoOp {
                action: "update".into(),
                path: key.as_str().into(),
                cid: Some(CidLink::from(*new_cid)),
                prev: Some(CidLink::from(*old_cid)),
            });
        }

        // Add deletes
        for (key, old_cid) in &self.deletes {
            ops.push(RepoOp {
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
    ///
    /// Uses an efficient walker-based algorithm that only visits changed subtrees.
    /// When two subtrees have the same CID, the entire subtree is skipped.
    pub async fn diff(&self, other: &Mst<S>) -> Result<MstDiff> {
        let mut diff = MstDiff::new();
        diff_recursive(self, other, &mut diff).await?;

        // Remove duplicate blocks: nodes that appear in both new_mst_blocks and removed_mst_blocks
        // are unchanged nodes that were traversed during the diff but shouldn't be counted as created/deleted.
        // This happens when we step into subtrees with different parent CIDs but encounter identical child nodes.
        let created_set: std::collections::HashSet<_> = diff.new_mst_blocks.keys().copied().collect();
        let removed_set: std::collections::HashSet<_> = diff.removed_mst_blocks.iter().copied().collect();
        let duplicates: std::collections::HashSet<_> = created_set.intersection(&removed_set).copied().collect();

        diff.new_mst_blocks.retain(|cid, _| !duplicates.contains(cid));
        diff.removed_mst_blocks.retain(|cid| !duplicates.contains(cid));

        Ok(diff)
    }
}

/// Recursively diff two MST nodes using cursors
fn diff_recursive<'a, S: BlockStore + Sync + 'static>(
    old: &'a Mst<S>,
    new: &'a Mst<S>,
    diff: &'a mut MstDiff,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        // If CIDs are equal, trees are identical - skip entire subtree
        let old_cid = old.get_pointer().await?;
        let new_cid = new.get_pointer().await?;
        if old_cid == new_cid {
            return Ok(());
        }

        // CIDs differ - use cursors to walk both trees
        let mut old_cursor = MstCursor::new(old.clone());
        let mut new_cursor = MstCursor::new(new.clone());

        // Don't advance yet - let loop handle roots like any other tree comparison
        loop {
            match (old_cursor.current(), new_cursor.current()) {
                (CursorPosition::End, CursorPosition::End) => break,

                // Only new entries remain - all adds
                (CursorPosition::End, CursorPosition::Leaf { key, cid }) => {
                    diff.creates.push((key.clone(), *cid));
                    diff.new_leaf_cids.push(*cid);
                    new_cursor.advance().await?;
                }
                (CursorPosition::End, CursorPosition::Tree { mst }) => {
                    track_added_tree(mst, diff).await?;
                    new_cursor.skip_subtree().await?;
                }

                // Only old entries remain - all deletes
                (CursorPosition::Leaf { key, cid }, CursorPosition::End) => {
                    diff.deletes.push((key.clone(), *cid));
                    diff.removed_cids.push(*cid);
                    old_cursor.advance().await?;
                }
                (CursorPosition::Tree { mst }, CursorPosition::End) => {
                    track_removed_tree(mst, diff).await?;
                    old_cursor.skip_subtree().await?;
                }

                // Both have entries - compare them
                (old_pos, new_pos) => {
                    // Handle Leaf/Leaf comparison FIRST (before layer checks)
                    // This matches rsky's logic - key comparison takes precedence
                    if let (
                        CursorPosition::Leaf {
                            key: old_key,
                            cid: old_cid,
                        },
                        CursorPosition::Leaf {
                            key: new_key,
                            cid: new_cid,
                        },
                    ) = (old_pos, new_pos)
                    {
                        match old_key.as_str().cmp(new_key.as_str()) {
                            std::cmp::Ordering::Equal => {
                                // Same key - check if value changed
                                if old_cid != new_cid {
                                    diff.updates.push((old_key.clone(), *new_cid, *old_cid));
                                    diff.new_leaf_cids.push(*new_cid);
                                    diff.removed_cids.push(*old_cid);
                                }
                                old_cursor.advance().await?;
                                new_cursor.advance().await?;
                            }
                            std::cmp::Ordering::Less => {
                                // Old key < new key - old was deleted
                                diff.deletes.push((old_key.clone(), *old_cid));
                                diff.removed_cids.push(*old_cid);
                                old_cursor.advance().await?;
                            }
                            std::cmp::Ordering::Greater => {
                                // Old key > new key - new was created
                                diff.creates.push((new_key.clone(), *new_cid));
                                diff.new_leaf_cids.push(*new_cid);
                                new_cursor.advance().await?;
                            }
                        }
                        continue;
                    }

                    // Now check layers for Tree comparisons
                    let old_layer = old_cursor.layer().await?;
                    let new_layer = new_cursor.layer().await?;

                    match (old_pos, new_pos) {
                        // Both trees at same layer - check if CIDs match, skip or recurse
                        (
                            CursorPosition::Tree { mst: old_tree },
                            CursorPosition::Tree { mst: new_tree },
                        ) if old_layer == new_layer => {
                            let old_tree_cid = old_tree.get_pointer().await?;
                            let new_tree_cid = new_tree.get_pointer().await?;

                            if old_tree_cid == new_tree_cid {
                                // Same subtree - skip both
                                old_cursor.skip_subtree().await?;
                                new_cursor.skip_subtree().await?;
                            } else {
                                // Different subtrees - serialize and track MST blocks, then step in to find leaf diff
                                serialize_and_track_mst(new_tree, diff).await?;
                                diff.removed_mst_blocks.push(old_tree_cid);
                                // Don't track recursively - step in to compare leaves
                                old_cursor.advance().await?;
                                new_cursor.advance().await?;
                            }
                        }

                        // Layer mismatch handling (rsky pattern)
                        _ if old_layer > new_layer => {
                            // Old is at higher layer - need to descend or advance appropriately
                            match old_pos {
                                CursorPosition::Leaf { .. } => {
                                    // Higher layer leaf - serialize and track new node, advance new to continue comparing
                                    if let CursorPosition::Tree { mst } = new_pos {
                                        serialize_and_track_mst(mst, diff).await?;
                                    }
                                    new_cursor.advance().await?; // Don't blindly add - let loop compare
                                }
                                CursorPosition::Tree { mst } => {
                                    // Higher layer tree - track MST block removal, then step into to find leaves
                                    let tree_cid = mst.get_pointer().await?;
                                    diff.removed_mst_blocks.push(tree_cid);
                                    old_cursor.advance().await?; // Step into to continue comparing
                                }
                                _ => {}
                            }
                        }

                        _ if old_layer < new_layer => {
                            // New is at higher layer
                            match new_pos {
                                CursorPosition::Leaf { .. } => {
                                    // Higher layer leaf - track old node, advance old to continue comparing
                                    if let CursorPosition::Tree { mst } = old_pos {
                                        let tree_cid = mst.get_pointer().await?;
                                        diff.removed_mst_blocks.push(tree_cid);
                                    }
                                    old_cursor.advance().await?; // Don't blindly delete - let loop compare
                                }
                                CursorPosition::Tree { mst } => {
                                    // Higher layer tree - serialize and track MST block addition, then step into to find leaves
                                    serialize_and_track_mst(mst, diff).await?;
                                    new_cursor.advance().await?; // Step into to continue comparing
                                }
                                _ => {}
                            }
                        }

                        // Same layer, mixed Leaf/Tree - step into tree to compare
                        (CursorPosition::Leaf { .. }, CursorPosition::Tree { mst }) => {
                            // Old has leaf, new has tree - serialize and track new MST block, step in to compare leaves
                            serialize_and_track_mst(mst, diff).await?;
                            new_cursor.advance().await?;
                        }

                        (CursorPosition::Tree { mst }, CursorPosition::Leaf { .. }) => {
                            // Old has tree, new has leaf - track removed MST block, step in to compare leaves
                            let tree_cid = mst.get_pointer().await?;
                            diff.removed_mst_blocks.push(tree_cid);
                            old_cursor.advance().await?;
                        }

                        _ => {}
                    }
                }
            }
        }

        Ok(())
    })
}

/// Serialize MST node and add to new_mst_blocks
async fn serialize_and_track_mst<S: BlockStore + Sync + 'static>(
    tree: &Mst<S>,
    diff: &mut MstDiff,
) -> Result<()> {
    let tree_cid = tree.get_pointer().await?;

    // Serialize the MST node
    let entries = tree.get_entries().await?;
    let node_data = serialize_node_data(&entries).await?;
    let cbor = serde_ipld_dagcbor::to_vec(&node_data)
        .map_err(|e| RepoError::serialization(e).with_context(format!("serializing MST node for diff tracking: {}", tree_cid)))?;

    // Track the serialized block
    diff.new_mst_blocks.insert(tree_cid, Bytes::from(cbor));

    Ok(())
}

/// Track entire tree as added (all leaves and nodes)
fn track_added_tree<'a, S: BlockStore + Sync + 'static>(
    tree: &'a Mst<S>,
    diff: &'a mut MstDiff,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        // Serialize and track this MST node
        serialize_and_track_mst(tree, diff).await?;

        let entries = tree.get_entries().await?;
        for entry in &entries {
            match entry {
                NodeEntry::Leaf { key, value } => {
                    diff.creates.push((key.clone(), *value));
                    diff.new_leaf_cids.push(*value);
                }
                NodeEntry::Tree(subtree) => {
                    track_added_tree(subtree, diff).await?;
                }
            }
        }

        Ok(())
    })
}

/// Track entire tree as removed (all leaves and nodes)
fn track_removed_tree<'a, S: BlockStore + Sync + 'static>(
    tree: &'a Mst<S>,
    diff: &'a mut MstDiff,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        // Track this MST node as removed
        let tree_cid = tree.get_pointer().await?;
        diff.removed_mst_blocks.push(tree_cid);

        // Recursively remove all leaves and nodes
        let entries = tree.get_entries().await?;
        for entry in &entries {
            match entry {
                NodeEntry::Leaf { key, value } => {
                    diff.deletes.push((key.clone(), *value));
                    diff.removed_cids.push(*value);
                }
                NodeEntry::Tree(subtree) => {
                    track_removed_tree(subtree, diff).await?;
                }
            }
        }

        Ok(())
    })
}

impl<S: BlockStore + Sync + 'static> Mst<S> {
    /// Compute diff from this tree to empty (all deletes)
    ///
    /// Returns diff representing deletion of all records in this tree.
    pub async fn diff_to_empty(&self) -> Result<MstDiff> {
        let mut diff = MstDiff::new();
        track_removed_tree_all(self, &mut diff).await?;
        Ok(diff)
    }
}

/// Track entire tree as removed (all nodes and leaves)
fn track_removed_tree_all<'a, S: BlockStore + Sync + 'static>(
    tree: &'a Mst<S>,
    diff: &'a mut MstDiff,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        // Track this node as removed
        let tree_cid = tree.get_pointer().await?;
        diff.removed_mst_blocks.push(tree_cid);

        // Recurse through entries
        let entries = tree.get_entries().await?;
        for entry in &entries {
            match entry {
                NodeEntry::Leaf { key, value } => {
                    diff.deletes.push((key.clone(), *value));
                    diff.removed_cids.push(*value);
                }
                NodeEntry::Tree(subtree) => {
                    track_removed_tree_all(subtree, diff).await?;
                }
            }
        }

        Ok(())
    })
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
        let tree2 = tree2.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree2 = tree2.add("com.example.test/b", test_cid(2)).await.unwrap();

        let diff = tree1.diff(&tree2).await.unwrap();

        assert_eq!(diff.creates.len(), 2);
        assert_eq!(diff.updates.len(), 0);
        assert_eq!(diff.deletes.len(), 0);
        assert_eq!(diff.op_count(), 2);

        // Check creates content
        assert!(
            diff.creates
                .iter()
                .any(|(k, c)| k == "com.example.test/a" && *c == test_cid(1))
        );
        assert!(
            diff.creates
                .iter()
                .any(|(k, c)| k == "com.example.test/b" && *c == test_cid(2))
        );
    }

    #[tokio::test]
    async fn test_diff_deletes() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("com.example.test/b", test_cid(2)).await.unwrap();

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
                .any(|(k, c)| k == "com.example.test/a" && *c == test_cid(1))
        );
        assert!(
            diff.deletes
                .iter()
                .any(|(k, c)| k == "com.example.test/b" && *c == test_cid(2))
        );
    }

    #[tokio::test]
    async fn test_diff_updates() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("com.example.test/b", test_cid(2)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("com.example.test/a", test_cid(10)).await.unwrap(); // Changed CID
        let tree2 = tree2.add("com.example.test/b", test_cid(2)).await.unwrap(); // Same CID

        let diff = tree1.diff(&tree2).await.unwrap();

        assert_eq!(diff.creates.len(), 0);
        assert_eq!(diff.updates.len(), 1); // Only "a" changed
        assert_eq!(diff.deletes.len(), 0);
        assert_eq!(diff.op_count(), 1);

        // Check update content
        assert_eq!(diff.updates[0].0, "com.example.test/a");
        assert_eq!(diff.updates[0].1, test_cid(10)); // new CID
        assert_eq!(diff.updates[0].2, test_cid(1)); // old CID
    }

    #[tokio::test]
    async fn test_diff_mixed_operations() {
        let storage1 = Arc::new(MemoryBlockStore::new());
        let tree1 = Mst::new(storage1);
        let tree1 = tree1.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("com.example.test/b", test_cid(2)).await.unwrap();
        let tree1 = tree1.add("com.example.test/c", test_cid(3)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("com.example.test/a", test_cid(10)).await.unwrap(); // Updated
        let tree2 = tree2.add("com.example.test/b", test_cid(2)).await.unwrap(); // Unchanged
        // "c" deleted
        let tree2 = tree2.add("com.example.test/d", test_cid(4)).await.unwrap(); // Created

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
        let tree = tree.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree = tree.add("com.example.test/b", test_cid(2)).await.unwrap();
        let tree = tree.add("com.example.test/c", test_cid(3)).await.unwrap();

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
        let tree1 = tree1.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree1 = tree1.add("com.example.test/b", test_cid(2)).await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let tree2 = Mst::new(storage2);
        let tree2 = tree2.add("com.example.test/b", test_cid(2)).await.unwrap();
        let tree2 = tree2.add("com.example.test/c", test_cid(3)).await.unwrap();

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
