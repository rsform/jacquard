//! MST cursor for efficient tree traversal

use super::node::NodeEntry;
use super::tree::Mst;
use crate::error::Result;
use crate::storage::BlockStore;
use cid::Cid as IpldCid;
use smol_str::SmolStr;

#[cfg(debug_assertions)]
use std::collections::HashSet;
#[cfg(debug_assertions)]
use std::sync::{Arc, RwLock};

/// Position within an MST traversal
#[derive(Debug, Clone)]
pub enum CursorPosition<S: BlockStore> {
    /// Pointing at a leaf entry
    Leaf {
        /// Leaf key
        key: SmolStr,
        /// Leaf CID (record value)
        cid: IpldCid,
    },

    /// Pointing at a tree (subtree root)
    Tree {
        /// Subtree MST
        mst: Mst<S>,
    },

    /// Past the end of traversal
    End,
}

/// Cursor for navigating an MST in sorted order
///
/// Maintains a position in the tree and supports efficient navigation:
/// - `advance()`: Move to next item in sorted order
/// - `skip_subtree()`: Skip entire subtree at current position
/// - `current()`: Get current position without moving
///
/// # Example traversal
///
/// ```ignore
/// let mut cursor = MstCursor::new(tree);
///
/// while !cursor.is_end() {
///     match cursor.current() {
///         CursorPosition::Leaf { key, cid } => {
///             println!("Leaf: {} -> {}", key, cid);
///         }
///         CursorPosition::Tree { mst } => {
///             println!("Subtree at layer {}", mst.layer);
///         }
///         CursorPosition::End => break,
///     }
///     cursor.advance().await?;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MstCursor<S: BlockStore> {
    /// Stack of (node, entries, index) pairs tracking path from root
    ///
    /// Each entry represents a level we've descended into:
    /// - `node`: The MST node at this level
    /// - `entries`: Cached entries for this node
    /// - `index`: Current position within entries
    path: Vec<(Mst<S>, Vec<NodeEntry<S>>, usize)>,

    /// Current position in traversal
    current: CursorPosition<S>,

    /// Track CIDs accessed during traversal (debug only)
    #[cfg(debug_assertions)]
    accessed_cids: Option<Arc<RwLock<HashSet<IpldCid>>>>,
}

impl<S: BlockStore + Sync + 'static> MstCursor<S> {
    /// Create new cursor at the start of a tree
    ///
    /// Initial position is the root of the tree (which is a Tree position).
    /// Call `advance()` to move to the first leaf.
    pub fn new(root: Mst<S>) -> Self {
        Self {
            path: Vec::new(),
            current: CursorPosition::Tree { mst: root },
            #[cfg(debug_assertions)]
            accessed_cids: None,
        }
    }

    /// Create new cursor with dirty tracking enabled
    ///
    /// Records all CIDs accessed during traversal in the provided set.
    #[cfg(debug_assertions)]
    pub fn new_with_tracking(root: Mst<S>, tracking: Arc<RwLock<HashSet<IpldCid>>>) -> Self {
        Self {
            path: Vec::new(),
            current: CursorPosition::Tree { mst: root },
            accessed_cids: Some(tracking),
        }
    }

    /// Get current position without advancing
    pub fn current(&self) -> &CursorPosition<S> {
        &self.current
    }

    /// Check if cursor is at end
    pub fn is_end(&self) -> bool {
        matches!(self.current, CursorPosition::End)
    }

    /// Get key at current position (if pointing at leaf)
    pub fn key(&self) -> Option<&str> {
        match &self.current {
            CursorPosition::Leaf { key, .. } => Some(key.as_str()),
            _ => None,
        }
    }

    /// Get the layer we're currently walking at
    ///
    /// Returns the layer of the node we're traversing within.
    /// If at the root level (before stepping in), returns root's layer + 1.
    pub async fn layer(&self) -> Result<usize> {
        if let Some((walking_node, _, _)) = self.path.last() {
            // Track CID access
            #[cfg(debug_assertions)]
            if let Some(ref tracking) = self.accessed_cids {
                if let Ok(cid) = walking_node.get_pointer().await {
                    tracking.write().unwrap().insert(cid);
                }
            }

            // We're inside a node - return its layer
            walking_node.get_layer().await
        } else {
            // At root level (not yet stepped in) - return root's layer + 1
            // This matches rsky's walker behavior: being "at" the root (before entering)
            // is one layer higher than being "inside" the root
            match &self.current {
                CursorPosition::Tree { mst } => {
                    // Track CID access
                    #[cfg(debug_assertions)]
                    if let Some(ref tracking) = self.accessed_cids {
                        if let Ok(cid) = mst.get_pointer().await {
                            tracking.write().unwrap().insert(cid);
                        }
                    }

                    let root_layer = mst.get_layer().await?;
                    Ok(root_layer + 1)
                }
                CursorPosition::End => Ok(0),
                CursorPosition::Leaf { .. } => {
                    // Shouldn't happen - root can't be a leaf
                    Ok(0)
                }
            }
        }
    }

    /// Advance to next position in sorted order
    ///
    /// - If at Leaf: move to next sibling or pop up
    /// - If at Tree: descend into it (step into first entry)
    /// - If at End: stay at End
    pub async fn advance(&mut self) -> Result<()> {
        match &self.current {
            CursorPosition::End => Ok(()),

            CursorPosition::Leaf { .. } => {
                // Move to next sibling
                self.step_over().await
            }

            CursorPosition::Tree { mst } => {
                // Descend into tree
                self.step_into(mst.clone()).await
            }
        }
    }

    /// Skip entire subtree at current position
    ///
    /// If pointing at a Tree, skips all its contents.
    /// If pointing at a Leaf, equivalent to `advance()`.
    pub async fn skip_subtree(&mut self) -> Result<()> {
        self.step_over().await
    }

    /// Move to next sibling or pop up
    fn step_over<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if let Some((_node, entries, index)) = self.path.last_mut() {
                // Try to move to next entry in current node
                *index += 1;

                if *index < entries.len() {
                    // Move to next entry
                    self.current = match &entries[*index] {
                        NodeEntry::Leaf { key, value } => CursorPosition::Leaf {
                            key: key.clone(),
                            cid: *value,
                        },
                        NodeEntry::Tree(tree) => CursorPosition::Tree { mst: tree.clone() },
                    };
                    Ok(())
                } else {
                    // No more entries at this level - pop up
                    self.path.pop();
                    self.step_over().await
                }
            } else {
                // No parent - we're done
                self.current = CursorPosition::End;
                Ok(())
            }
        })
    }

    /// Descend into a tree node
    async fn step_into(&mut self, mst: Mst<S>) -> Result<()> {
        // Track CID access
        #[cfg(debug_assertions)]
        if let Some(ref tracking) = self.accessed_cids {
            if let Ok(cid) = mst.get_pointer().await {
                tracking.write().unwrap().insert(cid);
            }
        }

        let entries = mst.get_entries().await?;

        if entries.is_empty() {
            // Empty tree - skip it
            self.step_over().await
        } else {
            // Push current level onto stack and move to first entry
            self.path.push((mst, entries.clone(), 0));

            self.current = match &entries[0] {
                NodeEntry::Leaf { key, value } => CursorPosition::Leaf {
                    key: key.clone(),
                    cid: *value,
                },
                NodeEntry::Tree(tree) => CursorPosition::Tree { mst: tree.clone() },
            };

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DAG_CBOR_CID_CODEC;
    use crate::mst::tree::Mst;
    use crate::storage::memory::MemoryBlockStore;
    use jacquard_common::types::crypto::SHA2_256;
    use std::sync::Arc;

    fn test_cid(n: u8) -> IpldCid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    #[tokio::test]
    async fn test_cursor_empty_tree() {
        let storage = Arc::new(MemoryBlockStore::new());
        let tree = Mst::new(storage);

        let mut cursor = MstCursor::new(tree);

        // Should start at root (Tree position)
        assert!(matches!(cursor.current(), CursorPosition::Tree { .. }));

        // Advance into empty tree should reach end
        cursor.advance().await.unwrap();
        assert!(cursor.is_end());
    }

    #[tokio::test]
    async fn test_cursor_single_leaf() {
        let storage = Arc::new(MemoryBlockStore::new());
        let tree = Mst::new(storage);
        let tree = tree.add("com.example.test/key1", test_cid(1)).await.unwrap();

        let mut cursor = MstCursor::new(tree);

        // Start at root
        assert!(matches!(cursor.current(), CursorPosition::Tree { .. }));

        // Advance to first leaf
        cursor.advance().await.unwrap();
        assert_eq!(cursor.key(), Some("com.example.test/key1"));

        // Advance past last leaf
        cursor.advance().await.unwrap();
        assert!(cursor.is_end());
    }

    #[tokio::test]
    async fn test_cursor_multiple_leaves() {
        let storage = Arc::new(MemoryBlockStore::new());
        let tree = Mst::new(storage);
        let tree = tree.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree = tree.add("com.example.test/b", test_cid(2)).await.unwrap();
        let tree = tree.add("com.example.test/c", test_cid(3)).await.unwrap();

        let mut cursor = MstCursor::new(tree);

        let mut keys = Vec::new();

        // Skip root
        cursor.advance().await.unwrap();

        while !cursor.is_end() {
            if let Some(key) = cursor.key() {
                keys.push(key.to_string());
            }
            cursor.advance().await.unwrap();
        }

        assert_eq!(keys, vec!["com.example.test/a", "com.example.test/b", "com.example.test/c"]);
    }

    #[tokio::test]
    async fn test_cursor_skip_subtree() {
        let storage = Arc::new(MemoryBlockStore::new());
        let tree = Mst::new(storage);

        // Add enough keys to create subtrees
        let tree = tree.add("com.example.test/a", test_cid(1)).await.unwrap();
        let tree = tree.add("com.example.test/b", test_cid(2)).await.unwrap();
        let tree = tree.add("com.example.test/c", test_cid(3)).await.unwrap();

        let mut cursor = MstCursor::new(tree);

        // Advance to first position
        cursor.advance().await.unwrap();

        // If we hit a tree, skip it
        let mut leaf_count = 0;
        while !cursor.is_end() {
            match cursor.current() {
                CursorPosition::Leaf { .. } => {
                    leaf_count += 1;
                    cursor.advance().await.unwrap();
                }
                CursorPosition::Tree { .. } => {
                    // Skip entire subtree
                    cursor.skip_subtree().await.unwrap();
                }
                CursorPosition::End => break,
            }
        }

        // We should have encountered some leaves
        assert!(leaf_count > 0);
    }
}
