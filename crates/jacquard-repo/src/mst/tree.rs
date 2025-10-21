//! Immutable Merkle Search Tree operations

use super::node::NodeEntry;
use super::util;
use crate::error::{RepoError, Result};
use crate::storage::BlockStore;
use cid::Cid as IpldCid;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Write operation for batch application
///
/// Represents a single operation to apply to an MST.
/// For firehose operations where `prev` may be optional (v3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOp {
    /// Create new record (error if exists)
    Create {
        /// Record key (collection/rkey)
        key: SmolStr,
        /// Record CID
        cid: IpldCid,
    },

    /// Update existing record (error if not exists)
    ///
    /// `prev` is optional in v3 (required in v2)
    Update {
        /// Record key (collection/rkey)
        key: SmolStr,
        /// New record CID
        cid: IpldCid,
        /// Previous CID (optional for validation)
        prev: Option<IpldCid>,
    },

    /// Delete record
    ///
    /// `prev` is optional in v3 (required in v2)
    Delete {
        /// Record key (collection/rkey)
        key: SmolStr,
        /// Previous CID (optional for validation)
        prev: Option<IpldCid>,
    },
}

/// Verified write operation with required prev fields
///
/// Used for operations where prev CID has been verified against tree state.
/// Safer than `WriteOp` because it always validates prev values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedWriteOp {
    /// Create new record (verified not to exist)
    Create {
        /// Record key (collection/rkey)
        key: SmolStr,
        /// Record CID
        cid: IpldCid,
    },

    /// Update existing record (with verified prev CID)
    Update {
        /// Record key (collection/rkey)
        key: SmolStr,
        /// New record CID
        cid: IpldCid,
        /// Previous CID (required, validated)
        prev: IpldCid,
    },

    /// Delete record (with verified current CID)
    Delete {
        /// Record key (collection/rkey)
        key: SmolStr,
        /// Previous CID (required, validated)
        prev: IpldCid,
    },
}

/// Immutable Merkle Search Tree
///
/// MST operations return new tree instances, leaving the original unchanged.
/// This enables versioning and safe concurrent access to different tree versions.
///
/// # Architecture
///
/// The tree uses a flat `Vec<NodeEntry>` representation in memory, where
/// `NodeEntry` is an enum of `Tree` (subtree) and `Leaf` (key-value pair).
///
/// Entries are interleaved: `[Tree, Leaf, Tree, Leaf, Leaf, Tree]` etc.
/// This representation makes operations simple (Vec slicing, splicing).
///
/// The wire format (CBOR) uses prefix compression and pointers (left/tree).
/// See `NodeData` and `TreeEntry` in node.rs for serialization format.
///
/// # Layer-based structure
///
/// Keys are hashed (SHA-256) and leading zero bits determine layer:
/// - More leading zeros = higher layer (deeper in tree)
/// - Layer = floor(leading_zeros / 2) for ~4 fanout
/// - Deterministic and insertion-order independent
#[derive(Debug, Clone)]
pub struct Mst<S: BlockStore> {
    /// Block storage for loading/saving nodes (shared via Arc)
    storage: Arc<S>,

    /// Flat list of entries (lazy-loaded, interior mutable)
    ///
    /// `None` means not yet loaded from storage.
    /// Empty `Vec` means tree has been loaded and has no entries.
    entries: Arc<RwLock<Option<Vec<NodeEntry<S>>>>>,

    /// CID pointer to this node in storage (interior mutable)
    pointer: Arc<RwLock<IpldCid>>,

    /// Whether pointer is stale (entries modified, interior mutable)
    ///
    /// When `true`, `pointer` doesn't match current `entries`.
    /// Call `get_pointer()` to recompute and update.
    outdated_pointer: Arc<RwLock<bool>>,

    /// Layer hint for this node
    ///
    /// `None` means layer unknown (will be computed from entries).
    /// Layer is the maximum layer of any key in this node.
    layer: Option<usize>,
}

impl<S: BlockStore + Sync + 'static> Mst<S> {
    /// Create empty MST
    pub fn new(storage: Arc<S>) -> Self {
        Self {
            storage,
            entries: Arc::new(RwLock::new(Some(Vec::new()))),
            pointer: Arc::new(RwLock::new(IpldCid::default())),
            outdated_pointer: Arc::new(RwLock::new(true)),
            layer: Some(0),
        }
    }

    /// Create MST with existing entries
    ///
    /// Used internally for tree operations.
    /// Computes CID from entries (doesn't persist to storage).
    pub(crate) async fn create(
        storage: Arc<S>,
        entries: Vec<NodeEntry<S>>,
        layer: Option<usize>,
    ) -> Result<Self> {
        // Serialize and compute CID (don't persist yet)
        let node_data = util::serialize_node_data(&entries).await?;
        let cbor =
            serde_ipld_dagcbor::to_vec(&node_data).map_err(|e| RepoError::serialization(e))?;
        let cid = util::compute_cid(&cbor)?;

        let mst = Self {
            storage,
            entries: Arc::new(RwLock::new(Some(entries))),
            pointer: Arc::new(RwLock::new(cid)),
            outdated_pointer: Arc::new(RwLock::new(false)),
            layer,
        };

        Ok(mst)
    }

    /// Load MST from CID (lazy)
    ///
    /// Doesn't actually load from storage until entries are accessed.
    pub fn load(storage: Arc<S>, cid: IpldCid, layer: Option<usize>) -> Self {
        Self {
            storage,
            entries: Arc::new(RwLock::new(None)), // Not loaded yet
            pointer: Arc::new(RwLock::new(cid)),
            outdated_pointer: Arc::new(RwLock::new(false)),
            layer,
        }
    }

    /// Create new tree with modified entries
    ///
    /// Returns a new Mst with updated entries. Marks pointer as outdated.
    async fn new_tree(&self, entries: Vec<NodeEntry<S>>) -> Result<Self> {
        Ok(Self {
            storage: self.storage.clone(),
            entries: Arc::new(RwLock::new(Some(entries))),
            pointer: self.pointer.clone(),
            outdated_pointer: Arc::new(RwLock::new(true)),
            layer: self.layer,
        })
    }

    /// Get entries (lazy load if needed)
    async fn get_entries(&self) -> Result<Vec<NodeEntry<S>>> {
        {
            let entries_guard = self.entries.read().await;
            if let Some(ref entries) = *entries_guard {
                return Ok(entries.clone());
            }
        }

        // Load from storage
        let pointer = *self.pointer.read().await;
        let node_bytes = self
            .storage
            .get(&pointer)
            .await?
            .ok_or_else(|| RepoError::not_found("MST node", &pointer))?;

        let node_data: super::node::NodeData =
            serde_ipld_dagcbor::from_slice(&node_bytes).map_err(|e| RepoError::serialization(e))?;

        let entries = util::deserialize_node_data(self.storage.clone(), &node_data, self.layer)?;

        // Cache the loaded entries
        {
            let mut entries_guard = self.entries.write().await;
            *entries_guard = Some(entries.clone());
        }

        Ok(entries)
    }

    /// Get CID pointer (recompute if outdated)
    ///
    /// Computes CID from current entries but doesn't persist to storage.
    /// Use `collect_blocks()` to gather blocks for persistence.
    pub async fn get_pointer(&self) -> Result<IpldCid> {
        let outdated = *self.outdated_pointer.read().await;
        if !outdated {
            return Ok(*self.pointer.read().await);
        }

        // Serialize and compute CID (don't persist yet)
        let entries = self.get_entries().await?;
        let node_data = util::serialize_node_data(&entries).await?;
        let cbor =
            serde_ipld_dagcbor::to_vec(&node_data).map_err(|e| RepoError::serialization(e))?;
        let cid = util::compute_cid(&cbor)?;

        // Update pointer and mark as fresh
        {
            let mut pointer_guard = self.pointer.write().await;
            *pointer_guard = cid;
        }
        {
            let mut outdated_guard = self.outdated_pointer.write().await;
            *outdated_guard = false;
        }

        Ok(cid)
    }

    /// Get root CID (alias for get_pointer)
    pub async fn root(&self) -> Result<IpldCid> {
        self.get_pointer().await
    }

    /// Get shared reference to the block storage
    ///
    /// Useful for CAR export and other operations that need direct storage access.
    pub fn storage(&self) -> &Arc<S> {
        &self.storage
    }

    /// Get the layer of this node
    ///
    /// Layer is the maximum layer of any leaf key in this node.
    /// For nodes with no leaves, recursively checks subtrees.
    fn get_layer<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(layer) = self.layer {
                return Ok(layer);
            }

            // Compute layer from entries
            let entries = self.get_entries().await?;

            // Find first leaf and get its layer
            for entry in &entries {
                if let NodeEntry::Leaf { key, .. } = entry {
                    let layer = util::layer_for_key(key.as_str());
                    return Ok(layer);
                }
            }

            // No leaves found - check first subtree
            for entry in &entries {
                if let NodeEntry::Tree(subtree) = entry {
                    let child_layer = subtree.get_layer().await?;
                    return Ok(child_layer + 1);
                }
            }

            // Empty tree
            Ok(0)
        })
    }

    /// Find index of first leaf >= key
    ///
    /// Returns `entries.len()` if all leaves are < key.
    fn find_gt_or_equal_leaf_index_in(entries: &[NodeEntry<S>], key: &str) -> usize {
        for (i, entry) in entries.iter().enumerate() {
            if let NodeEntry::Leaf { key: leaf_key, .. } = entry {
                if leaf_key.as_str() >= key {
                    return i;
                }
            }
        }

        entries.len()
    }

    /// Get a value by key
    pub fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<IpldCid>>> + Send + 'a>>
    {
        Box::pin(async move {
            util::validate_key(key)?;

            let entries = self.get_entries().await?;
            let index = Self::find_gt_or_equal_leaf_index_in(&entries, key);

            // Check if we found exact match
            if index < entries.len() {
                if let NodeEntry::Leaf {
                    key: leaf_key,
                    value,
                } = &entries[index]
                {
                    if leaf_key.as_str() == key {
                        return Ok(Some(*value));
                    }
                }
            }

            // Not found at this level - check subtree before this index
            if index > 0 {
                if let NodeEntry::Tree(subtree) = &entries[index - 1] {
                    return subtree.get(key).await;
                }
            }

            Ok(None)
        })
    }

    /// Add a key-value pair (returns new tree)
    pub fn add<'a>(
        &'a self,
        key: &'a str,
        cid: IpldCid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Mst<S>>> + Send + 'a>> {
        Box::pin(async move {
            util::validate_key(key)?;

            let key_layer = util::layer_for_key(key);
            let node_layer = self.get_layer().await?;
            let entries = self.get_entries().await?;

            if key_layer == node_layer {
                // Key belongs at this layer - insert here
                let index = Self::find_gt_or_equal_leaf_index_in(&entries, key);

                // Check if key already exists
                if index < entries.len() {
                    if let NodeEntry::Leaf { key: leaf_key, .. } = &entries[index] {
                        if leaf_key.as_str() == key {
                            // Key exists - replace by just inserting at same position
                            let mut new_entries = entries.clone();
                            new_entries[index] = NodeEntry::Leaf {
                                key: smol_str::SmolStr::new(key),
                                value: cid,
                            };
                            return self.new_tree(new_entries).await;
                        }
                    }
                }

                // Check entry before insertion point
                if index > 0 {
                    match &entries[index - 1] {
                        NodeEntry::Leaf { .. } => {
                            // Prev is Leaf - just splice in
                            self.splice_in(
                                NodeEntry::Leaf {
                                    key: smol_str::SmolStr::new(key),
                                    value: cid,
                                },
                                index,
                            )
                            .await
                        }
                        NodeEntry::Tree(subtree) => {
                            // Prev is Tree - split it around key
                            let (left, right) = subtree.split_around(key).await?;
                            self.replace_with_split(
                                index - 1,
                                left,
                                NodeEntry::Leaf {
                                    key: smol_str::SmolStr::new(key),
                                    value: cid,
                                },
                                right,
                            )
                            .await
                        }
                    }
                } else {
                    // At far left - splice in
                    self.splice_in(
                        NodeEntry::Leaf {
                            key: smol_str::SmolStr::new(key),
                            value: cid,
                        },
                        index,
                    )
                    .await
                }
            } else if key_layer < node_layer {
                // Key belongs on lower layer - recurse into subtree
                let index = Self::find_gt_or_equal_leaf_index_in(&entries, key);

                if index > 0 {
                    if let NodeEntry::Tree(prev_tree) = &entries[index - 1] {
                        // Prev is Tree - add to it
                        let new_subtree = prev_tree.add(key, cid).await?;
                        return self
                            .update_entry(index - 1, NodeEntry::Tree(new_subtree))
                            .await;
                    }
                }

                // No prev tree - create child and add to it
                let child = self.create_child().await?;
                let new_subtree = child.add(key, cid).await?;
                self.splice_in(NodeEntry::Tree(new_subtree), index).await
            } else {
                // Key belongs on higher layer - create parent layers
                let extra_layers = key_layer - node_layer;

                let (mut left, mut right) = self.split_around(key).await?;

                // Create intermediate layers if gap > 1
                for _ in 1..extra_layers {
                    if let Some(l) = left {
                        left = Some(l.create_parent().await?);
                    }
                    if let Some(r) = right {
                        right = Some(r.create_parent().await?);
                    }
                }

                // Build new root
                let mut new_entries = Vec::new();
                if let Some(l) = left {
                    new_entries.push(NodeEntry::Tree(l));
                }
                new_entries.push(NodeEntry::Leaf {
                    key: smol_str::SmolStr::new(key),
                    value: cid,
                });
                if let Some(r) = right {
                    new_entries.push(NodeEntry::Tree(r));
                }

                Mst::create(self.storage.clone(), new_entries, Some(key_layer)).await
            }
        })
    }

    /// Delete a key (returns new tree)
    pub fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Mst<S>>> + Send + 'a>> {
        Box::pin(async move {
            util::validate_key(key)?;

            let altered = self.delete_recurse(key).await?;
            altered.trim_top().await
        })
    }

    /// Recursively delete a key
    fn delete_recurse<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Mst<S>>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.get_entries().await?;
            let index = Self::find_gt_or_equal_leaf_index_in(&entries, key);

            // Check if found at this level
            if index < entries.len() {
                if let NodeEntry::Leaf { key: leaf_key, .. } = &entries[index] {
                    if leaf_key.as_str() == key {
                        // Found it - delete this entry
                        let prev = if index > 0 {
                            Some(&entries[index - 1])
                        } else {
                            None
                        };
                        let next = entries.get(index + 1);

                        // Check if we need to merge Trees
                        if let (
                            Some(NodeEntry::Tree(prev_tree)),
                            Some(NodeEntry::Tree(next_tree)),
                        ) = (prev, next)
                        {
                            // Merge the two Trees
                            let merged = prev_tree.append_merge(next_tree).await?;

                            // Build: [0..index-1] + [merged] + [index+2..]
                            let mut new_entries = entries[..index - 1].to_vec();
                            new_entries.push(NodeEntry::Tree(merged));
                            new_entries.extend_from_slice(&entries[index + 2..]);

                            return self.new_tree(new_entries).await;
                        }

                        // Simple case: just remove the entry
                        return self.remove_entry(index).await;
                    }
                }
            }

            // Not found at this level - recurse into prev Tree
            if index > 0 {
                if let NodeEntry::Tree(prev_tree) = &entries[index - 1] {
                    let subtree = prev_tree.delete_recurse(key).await?;
                    let subtree_entries = subtree.get_entries().await?;

                    if subtree_entries.is_empty() {
                        // Subtree is now empty - remove it
                        return self.remove_entry(index - 1).await;
                    } else {
                        // Update with new subtree
                        return self.update_entry(index - 1, NodeEntry::Tree(subtree)).await;
                    }
                }
            }

            // Key not found
            Err(RepoError::not_found("key", key))
        })
    }

    /// Update an existing key (returns new tree)
    pub async fn update(&self, key: &str, cid: IpldCid) -> Result<Mst<S>> {
        util::validate_key(key)?;

        // Check key exists
        if self.get(key).await?.is_none() {
            return Err(RepoError::not_found("key", key));
        }

        // Update is just add (which replaces)
        self.add(key, cid).await
    }

    /// Update entry at index
    async fn update_entry(&self, index: usize, entry: NodeEntry<S>) -> Result<Mst<S>> {
        let mut entries = self.get_entries().await?;
        entries[index] = entry;
        self.new_tree(entries).await
    }

    /// Remove entry at index
    async fn remove_entry(&self, index: usize) -> Result<Mst<S>> {
        let mut entries = self.get_entries().await?;
        entries.remove(index);
        self.new_tree(entries).await
    }

    /// Append entry to end
    async fn append(&self, entry: NodeEntry<S>) -> Result<Mst<S>> {
        let mut entries = self.get_entries().await?;
        entries.push(entry);
        self.new_tree(entries).await
    }

    /// Prepend entry to start
    async fn prepend(&self, entry: NodeEntry<S>) -> Result<Mst<S>> {
        let mut entries = self.get_entries().await?;
        entries.insert(0, entry);
        self.new_tree(entries).await
    }

    /// Splice in entry at index
    async fn splice_in(&self, entry: NodeEntry<S>, index: usize) -> Result<Mst<S>> {
        let mut entries = self.get_entries().await?;
        entries.insert(index, entry);
        self.new_tree(entries).await
    }

    /// Get slice of entries
    pub async fn slice(&self, start: usize, end: usize) -> Result<Vec<NodeEntry<S>>> {
        let entries = self.get_entries().await?;
        Ok(entries[start..end].to_vec())
    }

    /// Trim top node if it only contains one subtree
    fn trim_top(
        self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Mst<S>>> + Send>> {
        Box::pin(async move {
            let entries = self.get_entries().await?;

            if entries.len() == 1 {
                if let NodeEntry::Tree(subtree) = &entries[0] {
                    return subtree.clone().trim_top().await;
                }
            }

            Ok(self)
        })
    }

    /// Split tree around a key into left and right subtrees
    ///
    /// Returns (left, right) where:
    /// - left contains all entries < key
    /// - right contains all entries >= key
    ///
    /// Either side can be None if empty.
    pub fn split_around<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(Option<Mst<S>>, Option<Mst<S>>)>> + Send + 'a>,
    > {
        Box::pin(async move {
            let entries = self.get_entries().await?;
            let index = Self::find_gt_or_equal_leaf_index_in(&entries, key);

            // Split at index
            let left_data = entries[..index].to_vec();
            let right_data = entries[index..].to_vec();

            let mut left = self.new_tree(left_data.clone()).await?;
            let mut right = self.new_tree(right_data).await?;

            // If last entry in left is a Tree, recursively split it
            if let Some(NodeEntry::Tree(last_tree)) = left_data.last() {
                let left_len = left_data.len();
                left = left.remove_entry(left_len - 1).await?;

                let (split_left, split_right) = last_tree.split_around(key).await?;

                if let Some(sl) = split_left {
                    left = left.append(NodeEntry::Tree(sl)).await?;
                }
                if let Some(sr) = split_right {
                    right = right.prepend(NodeEntry::Tree(sr)).await?;
                }
            }

            // Return None for empty sides
            let left_out = if left.get_entries().await?.is_empty() {
                None
            } else {
                Some(left)
            };

            let right_out = if right.get_entries().await?.is_empty() {
                None
            } else {
                Some(right)
            };

            Ok((left_out, right_out))
        })
    }

    /// Merge two adjacent subtrees
    ///
    /// All keys in `to_merge` must be > all keys in `self`.
    /// Used primarily for delete operations.
    pub fn append_merge<'a>(
        &'a self,
        to_merge: &'a Mst<S>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Mst<S>>> + Send + 'a>> {
        Box::pin(async move {
            // Check same layer
            let self_layer = self.get_layer().await?;
            let merge_layer = to_merge.get_layer().await?;

            if self_layer != merge_layer {
                return Err(RepoError::invalid_mst(
                    "Cannot merge MST nodes from different layers",
                ));
            }

            let mut self_entries = self.get_entries().await?;
            let merge_entries = to_merge.get_entries().await?;

            // Check if we need to merge adjacent Trees
            let last_is_tree = matches!(self_entries.last(), Some(NodeEntry::Tree(_)));
            let first_is_tree = matches!(merge_entries.first(), Some(NodeEntry::Tree(_)));

            if last_is_tree && first_is_tree {
                // Both are Trees - recursively merge them
                if let (Some(NodeEntry::Tree(left_tree)), Some(NodeEntry::Tree(right_tree))) =
                    (self_entries.last(), merge_entries.first())
                {
                    let merged = left_tree.append_merge(right_tree).await?;

                    // Build new entries: self[..-1] + merged + merge[1..]
                    let mut new_entries = self_entries[..self_entries.len() - 1].to_vec();
                    new_entries.push(NodeEntry::Tree(merged));
                    new_entries.extend_from_slice(&merge_entries[1..]);

                    return self.new_tree(new_entries).await;
                }
            }

            // Simple case: just concatenate
            self_entries.extend(merge_entries);
            self.new_tree(self_entries).await
        })
    }

    /// Create empty child tree at layer-1
    pub async fn create_child(&self) -> Result<Mst<S>> {
        let layer = self.get_layer().await?;
        let child_layer = if layer > 0 { Some(layer - 1) } else { Some(0) };

        Mst::create(self.storage.clone(), Vec::new(), child_layer).await
    }

    /// Create parent tree at layer+1 containing self
    pub async fn create_parent(self) -> Result<Mst<S>> {
        let layer = self.get_layer().await?;

        Mst::create(
            self.storage.clone(),
            vec![NodeEntry::Tree(self)],
            Some(layer + 1),
        )
        .await
    }

    /// Replace entry at index with [left?, leaf, right?]
    async fn replace_with_split(
        &self,
        index: usize,
        left: Option<Mst<S>>,
        leaf: NodeEntry<S>,
        right: Option<Mst<S>>,
    ) -> Result<Mst<S>> {
        let entries = self.get_entries().await?;

        // Build: [0..index] + [left?] + [leaf] + [right?] + [index+1..]
        let mut new_entries = entries[..index].to_vec();

        if let Some(l) = left {
            new_entries.push(NodeEntry::Tree(l));
        }
        new_entries.push(leaf);
        if let Some(r) = right {
            new_entries.push(NodeEntry::Tree(r));
        }

        new_entries.extend_from_slice(&entries[index + 1..]);

        self.new_tree(new_entries).await
    }

    /// Get all leaf entries (key-CID pairs) in lexicographic order
    ///
    /// Recursively traverses the tree to collect all leaves.
    /// Used for diff calculation and tree listing.
    pub fn leaves<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<(smol_str::SmolStr, IpldCid)>>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            let mut result = Vec::new();
            self.collect_leaves(&mut result).await?;
            Ok(result)
        })
    }

    /// Recursively collect all leaves into the result vector
    fn collect_leaves<'a>(
        &'a self,
        result: &'a mut Vec<(smol_str::SmolStr, IpldCid)>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.get_entries().await?;

            for entry in entries {
                match entry {
                    NodeEntry::Tree(subtree) => {
                        // Recurse into subtree
                        subtree.collect_leaves(result).await?;
                    }
                    NodeEntry::Leaf { key, value } => {
                        // Add leaf to result
                        result.push((key, value));
                    }
                }
            }

            Ok(())
        })
    }

    /// Apply batch of verified write operations (returns new tree)
    ///
    /// More efficient than individual operations as it only rebuilds
    /// the tree structure once per operation. Operations are applied in order.
    ///
    /// # Validation
    ///
    /// - Create: errors if key already exists
    /// - Update: errors if key doesn't exist OR prev CID doesn't match
    /// - Delete: errors if key doesn't exist OR prev CID doesn't match
    ///
    /// All operations validate prev CIDs against current tree state.
    pub async fn batch(&self, ops: &[VerifiedWriteOp]) -> Result<Mst<S>> {
        let mut tree = self.clone();

        for op in ops {
            tree = match op {
                VerifiedWriteOp::Create { key, cid } => {
                    // Check doesn't exist
                    if tree.get(key.as_str()).await?.is_some() {
                        return Err(RepoError::invalid_mst(format!(
                            "Cannot create: key already exists: {}",
                            key
                        )));
                    }
                    tree.add(key.as_str(), *cid).await?
                }

                VerifiedWriteOp::Update { key, cid, prev } => {
                    // Check exists and validate prev
                    let current = tree
                        .get(key.as_str())
                        .await?
                        .ok_or_else(|| RepoError::not_found("key", key.as_str()))?;

                    if &current != prev {
                        return Err(RepoError::invalid_mst(format!(
                            "Update prev CID mismatch for key {}: expected {}, got {}",
                            key, prev, current
                        )));
                    }

                    tree.add(key.as_str(), *cid).await?
                }

                VerifiedWriteOp::Delete { key, prev } => {
                    // Check exists and validate prev
                    let current = tree
                        .get(key.as_str())
                        .await?
                        .ok_or_else(|| RepoError::not_found("key", key.as_str()))?;

                    if &current != prev {
                        return Err(RepoError::invalid_mst(format!(
                            "Delete prev CID mismatch for key {}: expected {}, got {}",
                            key, prev, current
                        )));
                    }

                    tree.delete(key.as_str()).await?
                }
            };
        }

        Ok(tree)
    }

    /// Collect all blocks that need persisting
    ///
    /// Recursively walks the tree, serializing nodes and collecting blocks
    /// that aren't already in storage. Skips nodes that are already persisted.
    ///
    /// Returns (root_cid, blocks) where blocks is a map of CID → bytes.
    pub fn collect_blocks<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<(IpldCid, std::collections::BTreeMap<IpldCid, bytes::Bytes>)>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            use bytes::Bytes;
            use std::collections::BTreeMap;

            let mut blocks = BTreeMap::new();
            let pointer = self.get_pointer().await?;

            // Check if already in storage
            if self.storage.has(&pointer).await? {
                return Ok((pointer, blocks));
            }

            // Serialize this node
            let entries = self.get_entries().await?;
            let node_data = util::serialize_node_data(&entries).await?;
            let cbor =
                serde_ipld_dagcbor::to_vec(&node_data).map_err(|e| RepoError::serialization(e))?;
            blocks.insert(pointer, Bytes::from(cbor));

            // Recursively collect from subtrees
            for entry in &entries {
                if let NodeEntry::Tree(subtree) = entry {
                    let (_, subtree_blocks) = subtree.collect_blocks().await?;
                    blocks.extend(subtree_blocks);
                }
            }

            Ok((pointer, blocks))
        })
    }

    /// Persist all unstored blocks to storage
    ///
    /// Convenience method that calls `collect_blocks()` and `put_many()`.
    /// Returns the root CID after persisting.
    pub async fn persist(&self) -> Result<IpldCid> {
        let (root_cid, blocks) = self.collect_blocks().await?;

        if !blocks.is_empty() {
            self.storage.put_many(blocks).await?;
        }

        Ok(root_cid)
    }

    /// Get all CIDs in the merkle path to a key
    ///
    /// Returns a list of CIDs representing the proof path from root to the target key:
    /// - Always includes the root CID (this node's pointer)
    /// - If key exists, includes the record CID
    /// - Includes all intermediate MST node CIDs in the path
    ///
    /// This is used for generating merkle proofs for record existence/non-existence.
    ///
    /// # Example
    ///
    /// For a key that exists:
    /// - Returns: `[root_cid, intermediate_node_cid?, ..., record_cid]`
    ///
    /// For a key that doesn't exist:
    /// - Returns: `[root_cid, intermediate_node_cid?, ...]` (proves absence)
    pub fn cids_for_path<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<IpldCid>>> + Send + 'a>>
    {
        Box::pin(async move {
            util::validate_key(key)?;

            let mut cids = vec![self.get_pointer().await?];
            let entries = self.get_entries().await?;
            let index = Self::find_gt_or_equal_leaf_index_in(&entries, key);

            // Check if we found exact match at this level
            if index < entries.len() {
                if let NodeEntry::Leaf {
                    key: leaf_key,
                    value,
                } = &entries[index]
                {
                    if leaf_key.as_str() == key {
                        cids.push(*value);
                        return Ok(cids);
                    }
                }
            }

            // Not found at this level - check subtree before this index
            if index > 0 {
                if let NodeEntry::Tree(subtree) = &entries[index - 1] {
                    let mut subtree_cids = subtree.cids_for_path(key).await?;
                    cids.append(&mut subtree_cids);
                    return Ok(cids);
                }
            }

            // Key not found in tree
            Ok(cids)
        })
    }

    /// Write all MST and record blocks to CAR writer
    ///
    /// Streams blocks directly to the writer as the tree is walked:
    /// - All MST node blocks (read from storage)
    /// - All leaf record blocks (read from storage)
    ///
    /// This is suitable for CAR export and avoids loading all blocks into memory.
    pub async fn write_blocks_to_car<W: tokio::io::AsyncWrite + Send + Unpin>(
        &self,
        writer: &mut iroh_car::CarWriter<W>,
    ) -> Result<()> {
        let mut leaf_cids = Vec::new();

        // Walk tree, writing MST nodes and collecting leaf CIDs
        self.write_mst_nodes_to_car(writer, &mut leaf_cids).await?;

        // Fetch and write all leaf record blocks
        let leaf_blocks = self.storage.get_many(&leaf_cids).await?;
        for (cid, maybe_data) in leaf_cids.iter().zip(leaf_blocks) {
            if let Some(data) = maybe_data {
                writer
                    .write(*cid, &data)
                    .await
                    .map_err(|e| RepoError::car(e))?;
            }
        }

        Ok(())
    }

    /// Recursively write MST nodes to CAR and collect leaf CIDs
    fn write_mst_nodes_to_car<'a, W: tokio::io::AsyncWrite + Send + Unpin>(
        &'a self,
        writer: &'a mut iroh_car::CarWriter<W>,
        leaf_cids: &'a mut Vec<IpldCid>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let pointer = self.get_pointer().await?;

            // Read MST node from storage and write to CAR
            let node_bytes = self
                .storage
                .get(&pointer)
                .await?
                .ok_or_else(|| RepoError::not_found("MST node", &pointer))?;

            writer
                .write(pointer, &node_bytes)
                .await
                .map_err(|e| RepoError::car(e))?;

            // Parse to get entries
            let entries = self.get_entries().await?;

            // Collect leaf CIDs and recurse into subtrees
            for entry in &entries {
                match entry {
                    NodeEntry::Leaf { value, .. } => {
                        leaf_cids.push(*value);
                    }
                    NodeEntry::Tree(subtree) => {
                        subtree.write_mst_nodes_to_car(writer, leaf_cids).await?;
                    }
                }
            }

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DAG_CBOR_CID_CODEC, storage::memory::MemoryBlockStore};
    use jacquard_common::types::crypto::SHA2_256;
    use smol_str::SmolStr;

    fn test_cid(n: u8) -> IpldCid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    #[tokio::test]
    async fn test_empty_tree() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let entries = mst.get_entries().await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_get_from_empty() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let result = mst.get("test/key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_manually_constructed_tree() {
        // Test with manually constructed entries (no CBOR)
        let storage = Arc::new(MemoryBlockStore::new());

        let entries = vec![
            NodeEntry::Leaf {
                key: SmolStr::new("a"),
                value: test_cid(1),
            },
            NodeEntry::Leaf {
                key: SmolStr::new("b"),
                value: test_cid(2),
            },
            NodeEntry::Leaf {
                key: SmolStr::new("c"),
                value: test_cid(3),
            },
        ];

        let mst = Mst::create(storage, entries, Some(0)).await.unwrap();

        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(1)));
        assert_eq!(mst.get("b").await.unwrap(), Some(test_cid(2)));
        assert_eq!(mst.get("c").await.unwrap(), Some(test_cid(3)));
        assert_eq!(mst.get("d").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_add_single_key() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let updated = mst.add("test/key", test_cid(1)).await.unwrap();

        assert_eq!(updated.get("test/key").await.unwrap(), Some(test_cid(1)));
    }

    #[tokio::test]
    async fn test_add_multiple_keys() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("a", test_cid(1)).await.unwrap();
        let mst = mst.add("b", test_cid(2)).await.unwrap();
        let mst = mst.add("c", test_cid(3)).await.unwrap();

        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(1)));
        assert_eq!(mst.get("b").await.unwrap(), Some(test_cid(2)));
        assert_eq!(mst.get("c").await.unwrap(), Some(test_cid(3)));
    }

    #[tokio::test]
    async fn test_add_replace_key() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("test", test_cid(1)).await.unwrap();
        let mst = mst.add("test", test_cid(2)).await.unwrap();

        assert_eq!(mst.get("test").await.unwrap(), Some(test_cid(2)));
    }

    #[tokio::test]
    async fn test_delete_single_key() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("test", test_cid(1)).await.unwrap();
        let mst = mst.delete("test").await.unwrap();

        assert_eq!(mst.get("test").await.unwrap(), None);
        assert_eq!(mst.get_entries().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_delete_from_multi_key_tree() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("a", test_cid(1)).await.unwrap();
        let mst = mst.add("b", test_cid(2)).await.unwrap();
        let mst = mst.add("c", test_cid(3)).await.unwrap();

        let mst = mst.delete("b").await.unwrap();

        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(1)));
        assert_eq!(mst.get("b").await.unwrap(), None);
        assert_eq!(mst.get("c").await.unwrap(), Some(test_cid(3)));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("a", test_cid(1)).await.unwrap();

        let result = mst.delete("b").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_serialization_roundtrip() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let mst = mst.add("a", test_cid(1)).await.unwrap();
        let mst = mst.add("b", test_cid(2)).await.unwrap();
        let mst = mst.add("c", test_cid(3)).await.unwrap();

        // Persist to storage
        let cid = mst.persist().await.unwrap();

        // Load from storage
        let reloaded = Mst::load(storage, cid, Some(0));

        // Verify all keys are present
        assert_eq!(reloaded.get("a").await.unwrap(), Some(test_cid(1)));
        assert_eq!(reloaded.get("b").await.unwrap(), Some(test_cid(2)));
        assert_eq!(reloaded.get("c").await.unwrap(), Some(test_cid(3)));
    }

    #[tokio::test]
    async fn test_cid_determinism() {
        // Same keys inserted in same order should produce same CID
        let storage1 = Arc::new(MemoryBlockStore::new());
        let mst1 = Mst::new(storage1);
        let mst1 = mst1.add("a", test_cid(1)).await.unwrap();
        let mst1 = mst1.add("b", test_cid(2)).await.unwrap();
        let mst1 = mst1.add("c", test_cid(3)).await.unwrap();
        let cid1 = mst1.get_pointer().await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let mst2 = Mst::new(storage2);
        let mst2 = mst2.add("a", test_cid(1)).await.unwrap();
        let mst2 = mst2.add("b", test_cid(2)).await.unwrap();
        let mst2 = mst2.add("c", test_cid(3)).await.unwrap();
        let cid2 = mst2.get_pointer().await.unwrap();

        assert_eq!(cid1, cid2);
    }

    #[tokio::test]
    async fn test_insertion_order_determinism() {
        // Different insertion orders should produce same CID
        let storage1 = Arc::new(MemoryBlockStore::new());
        let mst1 = Mst::new(storage1);
        let mst1 = mst1.add("a", test_cid(1)).await.unwrap();
        let mst1 = mst1.add("b", test_cid(2)).await.unwrap();
        let mst1 = mst1.add("c", test_cid(3)).await.unwrap();
        let cid1 = mst1.get_pointer().await.unwrap();

        let storage2 = Arc::new(MemoryBlockStore::new());
        let mst2 = Mst::new(storage2);
        let mst2 = mst2.add("c", test_cid(3)).await.unwrap();
        let mst2 = mst2.add("a", test_cid(1)).await.unwrap();
        let mst2 = mst2.add("b", test_cid(2)).await.unwrap();
        let cid2 = mst2.get_pointer().await.unwrap();

        assert_eq!(cid1, cid2);
    }

    #[tokio::test]
    async fn test_batch_creates() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let ops = vec![
            VerifiedWriteOp::Create {
                key: SmolStr::new("a"),
                cid: test_cid(1),
            },
            VerifiedWriteOp::Create {
                key: SmolStr::new("b"),
                cid: test_cid(2),
            },
            VerifiedWriteOp::Create {
                key: SmolStr::new("c"),
                cid: test_cid(3),
            },
        ];

        let mst = mst.batch(&ops).await.unwrap();

        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(1)));
        assert_eq!(mst.get("b").await.unwrap(), Some(test_cid(2)));
        assert_eq!(mst.get("c").await.unwrap(), Some(test_cid(3)));
    }

    #[tokio::test]
    async fn test_batch_mixed_operations() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        // Start with some keys
        let mst = mst.add("a", test_cid(1)).await.unwrap();
        let mst = mst.add("b", test_cid(2)).await.unwrap();
        let mst = mst.add("c", test_cid(3)).await.unwrap();

        let ops = vec![
            VerifiedWriteOp::Create {
                key: SmolStr::new("d"),
                cid: test_cid(4),
            },
            VerifiedWriteOp::Update {
                key: SmolStr::new("a"),
                cid: test_cid(10),
                prev: test_cid(1),
            },
            VerifiedWriteOp::Delete {
                key: SmolStr::new("b"),
                prev: test_cid(2),
            },
        ];

        let mst = mst.batch(&ops).await.unwrap();

        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(10))); // Updated
        assert_eq!(mst.get("b").await.unwrap(), None); // Deleted
        assert_eq!(mst.get("c").await.unwrap(), Some(test_cid(3))); // Unchanged
        assert_eq!(mst.get("d").await.unwrap(), Some(test_cid(4))); // Created
    }

    #[tokio::test]
    async fn test_batch_with_prev_validation() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);
        let mst = mst.add("a", test_cid(1)).await.unwrap();

        // Update with correct prev - should succeed
        let ops = vec![VerifiedWriteOp::Update {
            key: SmolStr::new("a"),
            cid: test_cid(2),
            prev: test_cid(1),
        }];
        let mst = mst.batch(&ops).await.unwrap();
        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(2)));

        // Update with wrong prev - should fail
        let ops = vec![VerifiedWriteOp::Update {
            key: SmolStr::new("a"),
            cid: test_cid(3),
            prev: test_cid(99), // Wrong CID
        }];
        assert!(mst.batch(&ops).await.is_err());

        // Delete with correct prev - should succeed
        let ops = vec![VerifiedWriteOp::Delete {
            key: SmolStr::new("a"),
            prev: test_cid(2),
        }];
        let mst = mst.batch(&ops).await.unwrap();
        assert_eq!(mst.get("a").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_batch_create_duplicate_error() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);
        let mst = mst.add("a", test_cid(1)).await.unwrap();

        let ops = vec![VerifiedWriteOp::Create {
            key: SmolStr::new("a"),
            cid: test_cid(2),
        }];

        // Should error because key already exists
        assert!(mst.batch(&ops).await.is_err());
    }

    #[tokio::test]
    async fn test_batch_update_nonexistent_error() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let ops = vec![VerifiedWriteOp::Update {
            key: SmolStr::new("a"),
            cid: test_cid(1),
            prev: test_cid(99), // Doesn't matter since key doesn't exist
        }];

        // Should error because key doesn't exist
        assert!(mst.batch(&ops).await.is_err());
    }

    #[tokio::test]
    async fn test_batch_delete_nonexistent_error() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let ops = vec![VerifiedWriteOp::Delete {
            key: SmolStr::new("a"),
            prev: test_cid(99), // Doesn't matter since key doesn't exist
        }];

        // Should error because key doesn't exist
        assert!(mst.batch(&ops).await.is_err());
    }

    #[tokio::test]
    async fn test_batch_empty() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);
        let mst = mst.add("a", test_cid(1)).await.unwrap();

        let ops = vec![];
        let mst = mst.batch(&ops).await.unwrap();

        // Should be unchanged
        assert_eq!(mst.get("a").await.unwrap(), Some(test_cid(1)));
    }

    #[tokio::test]
    async fn test_cids_for_path_simple() {
        // Test cids_for_path with a simple flat tree
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("a", test_cid(1)).await.unwrap();
        let mst = mst.add("b", test_cid(2)).await.unwrap();
        let mst = mst.add("c", test_cid(3)).await.unwrap();

        // Get proof path for key "b"
        let cids = mst.cids_for_path("b").await.unwrap();

        // Should contain: root CID, record CID
        assert_eq!(cids.len(), 2);
        assert_eq!(cids[0], mst.get_pointer().await.unwrap());
        assert_eq!(cids[1], test_cid(2));
    }

    #[tokio::test]
    async fn test_cids_for_path_nonexistent() {
        // Test cids_for_path with a key that doesn't exist
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        let mst = mst.add("a", test_cid(1)).await.unwrap();
        let mst = mst.add("c", test_cid(3)).await.unwrap();

        // Get proof path for nonexistent key "b"
        let cids = mst.cids_for_path("b").await.unwrap();

        // Should contain root CID first, and NOT contain the record CID (proves absence)
        assert!(cids.len() >= 1, "Should have at least the root CID");
        assert_eq!(
            cids[0],
            mst.get_pointer().await.unwrap(),
            "First CID should be root"
        );
        assert!(
            !cids.contains(&test_cid(2)),
            "Should not contain nonexistent record"
        );
    }

    #[tokio::test]
    async fn test_cids_for_path_multiple_records() {
        // Test with multiple records across different collection paths (like rsky)
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        // Simulate records from different collections (app.bsky.feed.post, app.bsky.feed.like)
        let keys = vec![
            "app.bsky.feed.post/3l4qpz7ajrc2a",
            "app.bsky.feed.post/3l4qpz7bjrc2b",
            "app.bsky.feed.like/3l4qpz7cjrc2c",
            "app.bsky.feed.like/3l4qpz7djrc2d",
            "app.bsky.graph.follow/3l4qpz7ejrc2e",
        ];

        let mut mst = mst;
        for (i, key) in keys.iter().enumerate() {
            mst = mst.add(key, test_cid((i + 1) as u8)).await.unwrap();
        }

        // Get proof for each record
        for (i, key) in keys.iter().enumerate() {
            let cids = mst.cids_for_path(key).await.unwrap();

            // Should have root CID first
            assert_eq!(cids[0], mst.get_pointer().await.unwrap());

            // Should have record CID last (since record exists)
            assert_eq!(*cids.last().unwrap(), test_cid((i + 1) as u8));

            // Should have at least root + record
            assert!(cids.len() >= 2);
        }
    }

    #[tokio::test]
    async fn test_cids_for_path_proves_nonexistence() {
        // Test that we can prove a record doesn't exist in a tree with many records
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        // Add several records
        let existing_keys = vec![
            "com.example.posts/key1",
            "com.example.posts/key2",
            "com.example.posts/key4",
            "com.example.posts/key5",
        ];

        let mut mst = mst;
        for (i, key) in existing_keys.iter().enumerate() {
            mst = mst.add(key, test_cid((i + 1) as u8)).await.unwrap();
        }

        // Prove key3 doesn't exist (between key2 and key4)
        let nonexistent_key = "com.example.posts/key3";
        let cids = mst.cids_for_path(nonexistent_key).await.unwrap();

        // Should have root CID
        assert_eq!(cids[0], mst.get_pointer().await.unwrap());

        // Should NOT contain a record CID for key3
        assert!(!cids.contains(&test_cid(3)));

        // Proof is just the path showing where key3 WOULD be (proves absence)
        assert!(cids.len() >= 1);
    }

    #[tokio::test]
    async fn test_cids_for_path_collection_structure() {
        // Test proof generation for realistic collection/rkey structure
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage);

        // Simulate a repo with multiple collections and records
        let records = vec![
            ("com.atproto.repo.strongRef", "abc123", test_cid(1)),
            ("app.bsky.feed.post", "post1", test_cid(2)),
            ("app.bsky.feed.post", "post2", test_cid(3)),
            ("app.bsky.feed.like", "like1", test_cid(4)),
            ("app.bsky.graph.follow", "follow1", test_cid(5)),
        ];

        let mut mst = mst;
        for (collection, rkey, cid) in &records {
            let key = format!("{}/{}", collection, rkey);
            mst = mst.add(&key, *cid).await.unwrap();
        }

        // Persist to storage so we have real MST blocks
        let root_cid = mst.persist().await.unwrap();
        assert_eq!(root_cid, mst.get_pointer().await.unwrap());

        // Get proofs for each record
        for (collection, rkey, expected_cid) in &records {
            let key = format!("{}/{}", collection, rkey);
            let cids = mst.cids_for_path(&key).await.unwrap();

            // Verify structure
            assert_eq!(cids[0], root_cid, "First CID should be root");
            assert_eq!(
                *cids.last().unwrap(),
                *expected_cid,
                "Last CID should be record"
            );
        }
    }
}
