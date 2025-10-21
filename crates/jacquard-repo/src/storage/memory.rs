//! In-memory block storage implementation

use crate::error::Result;
use crate::storage::BlockStore;
use bytes::Bytes;
use cid::Cid as IpldCid;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// In-memory block storage using BTreeMap
///
/// Useful for:
/// - Testing
/// - Temporary operations
/// - Small repositories that fit in memory
///
/// Uses `Bytes` for efficient reference-counted storage with cheap cloning.
///
/// # Example
///
/// ```rust,ignore
/// use jacquard_repo::storage::{BlockStore, MemoryBlockStore};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let storage = MemoryBlockStore::new();
///
/// let data = b"hello world";
/// let cid = storage.put(data).await?;
///
/// let retrieved = storage.get(&cid).await?;
/// assert_eq!(retrieved.as_deref(), Some(&data[..]));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MemoryBlockStore {
    blocks: Arc<RwLock<BTreeMap<IpldCid, Bytes>>>,
}

impl MemoryBlockStore {
    /// Create new empty memory store
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Create new memory store from a map of blocks
    pub fn new_from_blocks(blocks: BTreeMap<IpldCid, Bytes>) -> Self {
        Self {
            blocks: Arc::new(RwLock::new(blocks)),
        }
    }

    /// Get number of blocks stored
    pub fn len(&self) -> usize {
        self.blocks.read().unwrap().len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.blocks.read().unwrap().is_empty()
    }

    /// Clear all blocks
    pub fn clear(&self) {
        self.blocks.write().unwrap().clear();
    }

    /// Put a block with a pre-computed CID (for testing)
    ///
    /// # Note
    ///
    /// This bypasses CID verification. Only use for testing.
    #[cfg(test)]
    pub(crate) async fn put_with_cid(&self, cid: IpldCid, data: impl Into<Bytes>) -> Result<()> {
        self.blocks.write().unwrap().insert(cid, data.into());
        Ok(())
    }
}

impl Default for MemoryBlockStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockStore for MemoryBlockStore {
    async fn get(&self, cid: &IpldCid) -> Result<Option<Bytes>> {
        Ok(self.blocks.read().unwrap().get(cid).cloned())
    }

    async fn put(&self, data: &[u8]) -> Result<IpldCid> {
        let cid = crate::mst::util::compute_cid(data)?;
        self.blocks
            .write()
            .unwrap()
            .insert(cid, Bytes::copy_from_slice(data));
        Ok(cid)
    }

    async fn has(&self, cid: &IpldCid) -> Result<bool> {
        Ok(self.blocks.read().unwrap().contains_key(cid))
    }

    async fn put_many(&self, blocks: impl IntoIterator<Item = (IpldCid, Bytes)> + Send) -> Result<()> {
        let mut store = self.blocks.write().unwrap();
        for (cid, data) in blocks {
            store.insert(cid, data);
        }
        Ok(())
    }

    async fn get_many(&self, cids: &[IpldCid]) -> Result<Vec<Option<Bytes>>> {
        let store = self.blocks.read().unwrap();
        let mut results = Vec::with_capacity(cids.len());
        for cid in cids {
            results.push(store.get(cid).cloned());
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_put_and_get() {
        let store = MemoryBlockStore::new();
        let data = b"test data";

        let cid = store.put(data).await.unwrap();
        let retrieved = store.get(&cid).await.unwrap();

        assert_eq!(retrieved.as_deref(), Some(&data[..]));
    }

    #[tokio::test]
    async fn test_has() {
        let store = MemoryBlockStore::new();
        let data = b"test data";

        let cid = store.put(data).await.unwrap();
        assert!(store.has(&cid).await.unwrap());

        let fake_cid = IpldCid::default();
        assert!(!store.has(&fake_cid).await.unwrap());
    }

    #[tokio::test]
    async fn test_put_many() {
        let store = MemoryBlockStore::new();

        let data1 = b"data1";
        let data2 = b"data2";
        let cid1 = crate::mst::util::compute_cid(data1).unwrap();
        let cid2 = crate::mst::util::compute_cid(data2).unwrap();

        store
            .put_many(vec![
                (cid1, Bytes::from_static(data1)),
                (cid2, Bytes::from_static(data2)),
            ])
            .await
            .unwrap();

        assert_eq!(store.len(), 2);
        assert!(store.has(&cid1).await.unwrap());
        assert!(store.has(&cid2).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_many() {
        let store = MemoryBlockStore::new();

        let data1 = b"data1";
        let data2 = b"data2";
        let cid1 = store.put(data1).await.unwrap();
        let cid2 = store.put(data2).await.unwrap();
        let fake_cid = IpldCid::default();

        let results = store.get_many(&[cid1, fake_cid, cid2]).await.unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_deref(), Some(&data1[..]));
        assert_eq!(results[1], None);
        assert_eq!(results[2].as_deref(), Some(&data2[..]));
    }

    #[tokio::test]
    async fn test_clear() {
        let store = MemoryBlockStore::new();
        store.put(b"data").await.unwrap();

        assert_eq!(store.len(), 1);
        store.clear();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn test_clone_shares_storage() {
        let store1 = MemoryBlockStore::new();
        let store2 = store1.clone();

        let cid = store1.put(b"test").await.unwrap();
        assert!(store2.has(&cid).await.unwrap());
    }
}
