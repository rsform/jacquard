//! Layered block storage for efficient firehose validation
//!
//! Provides a two-layer storage that reads from a writable layer first,
//! then falls back to a read-only base layer. All writes go to the writable layer.
//!
//! This is used for firehose validation to avoid copying the entire previous MST tree.

use crate::error::Result;
use crate::storage::BlockStore;
use bytes::Bytes;
use cid::Cid as IpldCid;

/// Layered block storage with a writable overlay and read-only base
///
/// Reads check the writable layer first, then fall back to the base layer.
/// All writes go to the writable layer only.
///
/// # Use Case
///
/// Firehose validation needs to:
/// 1. Load previous MST state from existing storage (base layer)
/// 2. Apply new operations that create new MST nodes (writable layer)
/// 3. Not pollute the base storage with temporary validation blocks
///
/// Without layering, we'd need to copy all previous MST blocks to temporary storage.
/// With layering, we just overlay temp storage on top of base storage.
///
/// # Example
///
/// ```rust,ignore
/// use jacquard_repo::storage::{LayeredBlockStore, MemoryBlockStore};
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let base = Arc::new(MemoryBlockStore::new()); // existing repo storage
/// let writable = MemoryBlockStore::new(); // temp storage for validation
///
/// let layered = LayeredBlockStore::new(writable, base);
///
/// // Reads check writable first, then base
/// // Writes only go to writable
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct LayeredBlockStore<W: BlockStore, B: BlockStore> {
    writable: W,
    base: std::sync::Arc<B>,
}

impl<W: BlockStore, B: BlockStore> LayeredBlockStore<W, B> {
    /// Create a new layered storage
    ///
    /// - `writable`: Top layer receiving all writes
    /// - `base`: Bottom layer for fallback reads (read-only, Arc-wrapped to avoid cloning)
    pub fn new(writable: W, base: std::sync::Arc<B>) -> Self {
        Self { writable, base }
    }

    /// Get reference to the writable layer
    pub fn writable(&self) -> &W {
        &self.writable
    }

    /// Get reference to the base layer
    pub fn base(&self) -> &std::sync::Arc<B> {
        &self.base
    }
}

impl<W: BlockStore + Sync + 'static, B: BlockStore + Sync + 'static> BlockStore
    for LayeredBlockStore<W, B>
{
    async fn get(&self, cid: &IpldCid) -> Result<Option<Bytes>> {
        // Check writable layer first
        if let Some(data) = self.writable.get(cid).await? {
            return Ok(Some(data));
        }

        // Fall back to base layer
        self.base.get(cid).await
    }

    async fn put(&self, data: &[u8]) -> Result<IpldCid> {
        // All writes go to writable layer
        self.writable.put(data).await
    }

    async fn has(&self, cid: &IpldCid) -> Result<bool> {
        // Check writable first
        if self.writable.has(cid).await? {
            return Ok(true);
        }

        // Fall back to base
        self.base.has(cid).await
    }

    async fn put_many(&self, blocks: impl IntoIterator<Item = (IpldCid, Bytes)> + Send) -> Result<()> {
        // All writes go to writable layer
        self.writable.put_many(blocks).await
    }

    async fn get_many(&self, cids: &[IpldCid]) -> Result<Vec<Option<Bytes>>> {
        let mut results = Vec::with_capacity(cids.len());

        for cid in cids {
            results.push(self.get(cid).await?);
        }

        Ok(results)
    }

    async fn apply_commit(&self, commit: crate::repo::CommitData) -> Result<()> {
        // All operations go to writable layer only (base layer is read-only)
        self.writable.apply_commit(commit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryBlockStore;

    #[tokio::test]
    async fn test_layered_read_from_writable() {
        let base = std::sync::Arc::new(MemoryBlockStore::new());
        let writable = MemoryBlockStore::new();

        // Put data in writable layer
        let cid = writable.put(b"test data").await.unwrap();

        let layered = LayeredBlockStore::new(writable, base);

        // Should read from writable layer
        let data = layered.get(&cid).await.unwrap();
        assert_eq!(&*data.unwrap(), b"test data");
    }

    #[tokio::test]
    async fn test_layered_fallback_to_base() {
        let base = std::sync::Arc::new(MemoryBlockStore::new());
        let writable = MemoryBlockStore::new();

        // Put data in base layer
        let cid = base.put(b"base data").await.unwrap();

        let layered = LayeredBlockStore::new(writable, base);

        // Should fall back to base layer
        let data = layered.get(&cid).await.unwrap();
        assert_eq!(&*data.unwrap(), b"base data");
    }

    #[tokio::test]
    async fn test_layered_writable_overrides_base() {
        let base = std::sync::Arc::new(MemoryBlockStore::new());
        let writable = MemoryBlockStore::new();

        // Put same content in both layers (will have same CID)
        let cid = base.put(b"original").await.unwrap();
        let cid2 = writable.put(b"original").await.unwrap();
        assert_eq!(cid, cid2); // Same content = same CID

        // Now put different content with manual override (in real usage this wouldn't happen,
        // but testing the layer priority)
        // Actually, we can't manually set CIDs, so let's test differently:

        // Put different data in each layer
        let base_cid = base.put(b"base content").await.unwrap();
        let writable_cid = writable.put(b"writable content").await.unwrap();

        let layered = LayeredBlockStore::new(writable, base);

        // Should get writable content for writable CID
        let data1 = layered.get(&writable_cid).await.unwrap().unwrap();
        assert_eq!(&*data1, b"writable content");

        // Should get base content for base CID
        let data2 = layered.get(&base_cid).await.unwrap().unwrap();
        assert_eq!(&*data2, b"base content");
    }

    #[tokio::test]
    async fn test_layered_writes_to_writable_only() {
        let base = std::sync::Arc::new(MemoryBlockStore::new());
        let writable = MemoryBlockStore::new();

        let layered = LayeredBlockStore::new(writable.clone(), base.clone());

        // Write through layered storage
        let cid = layered.put(b"new data").await.unwrap();

        // Should be in writable layer
        assert!(writable.has(&cid).await.unwrap());

        // Should NOT be in base layer
        assert!(!base.has(&cid).await.unwrap());
    }

    #[tokio::test]
    async fn test_layered_has_checks_both_layers() {
        let base = std::sync::Arc::new(MemoryBlockStore::new());
        let writable = MemoryBlockStore::new();

        let base_cid = base.put(b"base").await.unwrap();
        let writable_cid = writable.put(b"writable").await.unwrap();

        let layered = LayeredBlockStore::new(writable, base);

        // Should find in both layers
        assert!(layered.has(&base_cid).await.unwrap());
        assert!(layered.has(&writable_cid).await.unwrap());
    }
}
