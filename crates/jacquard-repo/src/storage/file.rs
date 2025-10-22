//! CAR file-backed block storage

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use cid::Cid as IpldCid;

use crate::error::Result;
use crate::storage::BlockStore;

/// CAR file-backed block storage
///
/// Loads entire CAR file into memory on construction, writes back on flush.
/// For very large CAR files, consider database-backed storage instead.
///
/// Primarily useful for testing and simple file-based persistence.
#[derive(Debug, Clone)]
pub struct FileBlockStore {
    path: PathBuf,
    blocks: Arc<RwLock<BTreeMap<IpldCid, Bytes>>>,
    roots: Arc<RwLock<Vec<IpldCid>>>,
    dirty: Arc<RwLock<bool>>,
}

impl FileBlockStore {
    /// Load from existing CAR file
    pub async fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        // Read header to get roots
        let roots = crate::car::read_car_header(&path).await?;

        // Read all blocks
        let blocks = crate::car::read_car(&path).await?;

        Ok(Self {
            path,
            blocks: Arc::new(RwLock::new(blocks)),
            roots: Arc::new(RwLock::new(roots)),
            dirty: Arc::new(RwLock::new(false)),
        })
    }

    /// Create new CAR file storage (empty)
    ///
    /// Creates an empty in-memory storage that will write to the given path
    /// when `flush()` is called.
    ///
    /// The file is not created until the first flush.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            blocks: Arc::new(RwLock::new(BTreeMap::new())),
            roots: Arc::new(RwLock::new(Vec::new())),
            dirty: Arc::new(RwLock::new(false)),
        }
    }

    /// Get the CAR file roots
    ///
    /// In a repository CAR file, roots typically contain the commit CID(s).
    pub fn roots(&self) -> Vec<IpldCid> {
        self.roots.read().unwrap().clone()
    }

    /// Set the CAR file roots (for writing)
    ///
    /// This marks the storage as dirty. Call `flush()` to persist the change.
    pub fn set_roots(&self, new_roots: Vec<IpldCid>) {
        *self.roots.write().unwrap() = new_roots;
        *self.dirty.write().unwrap() = true;
    }

    /// Write blocks back to CAR file if dirty
    ///
    /// This is an async operation that writes the entire block store to the
    /// CAR file. Only writes if there have been changes since the last flush.
    ///
    /// # Errors
    ///
    /// Returns an error if the CAR file cannot be written.
    pub async fn flush(&self) -> Result<()> {
        if !*self.dirty.read().unwrap() {
            return Ok(());
        }

        let blocks = self.blocks.read().unwrap().clone();
        let roots = self.roots.read().unwrap().clone();
        crate::car::write_car(&self.path, roots, blocks).await?;

        *self.dirty.write().unwrap() = false;
        Ok(())
    }

    /// Check if store has unflushed changes
    pub fn is_dirty(&self) -> bool {
        *self.dirty.read().unwrap()
    }

    /// Get the path to the CAR file
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl BlockStore for FileBlockStore {
    async fn get(&self, cid: &IpldCid) -> Result<Option<Bytes>> {
        Ok(self.blocks.read().unwrap().get(cid).cloned())
    }

    async fn put(&self, data: &[u8]) -> Result<IpldCid> {
        let cid = crate::mst::util::compute_cid(data)?;
        self.blocks
            .write()
            .unwrap()
            .insert(cid, Bytes::copy_from_slice(data));
        *self.dirty.write().unwrap() = true;
        Ok(cid)
    }

    async fn has(&self, cid: &IpldCid) -> Result<bool> {
        Ok(self.blocks.read().unwrap().contains_key(cid))
    }

    async fn put_many(
        &self,
        blocks: impl IntoIterator<Item = (IpldCid, Bytes)> + Send,
    ) -> Result<()> {
        let mut store = self.blocks.write().unwrap();
        for (cid, data) in blocks {
            store.insert(cid, data);
        }
        *self.dirty.write().unwrap() = true;
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

    async fn apply_commit(&self, commit: crate::repo::CommitData) -> Result<()> {
        let mut store = self.blocks.write().unwrap();

        // First, insert all new blocks
        for (cid, data) in commit.blocks {
            store.insert(cid, data);
        }

        // Then, delete all garbage-collected blocks
        for cid in commit.deleted_cids {
            store.remove(&cid);
        }

        *self.dirty.write().unwrap() = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DAG_CBOR_CID_CODEC;
    use jacquard_common::types::crypto::SHA2_256;
    use tempfile::NamedTempFile;

    fn test_cid(n: u8) -> IpldCid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    #[tokio::test]
    async fn test_new_empty_store() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = FileBlockStore::new(temp_file.path());

        assert!(storage.roots().is_empty());
        assert!(!storage.is_dirty());
        assert_eq!(storage.path(), temp_file.path());
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = FileBlockStore::new(temp_file.path());

        let data = b"test data";
        let cid = storage.put(data).await.unwrap();

        assert!(storage.is_dirty());

        let retrieved = storage.get(&cid).await.unwrap().unwrap();
        assert_eq!(retrieved.as_ref(), data);
    }

    #[tokio::test]
    async fn test_has() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = FileBlockStore::new(temp_file.path());

        let data = b"test data";
        let cid = storage.put(data).await.unwrap();

        assert!(storage.has(&cid).await.unwrap());
        assert!(!storage.has(&test_cid(99)).await.unwrap());
    }

    #[tokio::test]
    async fn test_flush_and_reload() {
        let temp_file = NamedTempFile::new().unwrap();

        // Create store, add data, flush
        let storage = FileBlockStore::new(temp_file.path());
        let data1 = b"test data 1";
        let data2 = b"test data 2";
        let cid1 = storage.put(data1).await.unwrap();
        let cid2 = storage.put(data2).await.unwrap();

        storage.set_roots(vec![cid1]);
        assert!(storage.is_dirty());

        storage.flush().await.unwrap();
        assert!(!storage.is_dirty());

        // Reload from file
        let storage2 = FileBlockStore::load(temp_file.path()).await.unwrap();

        assert_eq!(storage2.roots(), vec![cid1]);
        assert_eq!(storage2.get(&cid1).await.unwrap().unwrap().as_ref(), data1);
        assert_eq!(storage2.get(&cid2).await.unwrap().unwrap().as_ref(), data2);
        assert!(!storage2.is_dirty());
    }

    #[tokio::test]
    async fn test_put_many() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = FileBlockStore::new(temp_file.path());

        let data1 = Bytes::from_static(b"data 1");
        let data2 = Bytes::from_static(b"data 2");
        let cid1 = test_cid(1);
        let cid2 = test_cid(2);

        storage
            .put_many(vec![(cid1, data1.clone()), (cid2, data2.clone())])
            .await
            .unwrap();

        assert!(storage.is_dirty());
        assert_eq!(storage.get(&cid1).await.unwrap().unwrap(), data1);
        assert_eq!(storage.get(&cid2).await.unwrap().unwrap(), data2);
    }

    #[tokio::test]
    async fn test_get_many() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = FileBlockStore::new(temp_file.path());

        let data1 = b"data 1";
        let data2 = b"data 2";
        let cid1 = storage.put(data1).await.unwrap();
        let cid2 = storage.put(data2).await.unwrap();
        let cid3 = test_cid(99); // Non-existent

        let results = storage.get_many(&[cid1, cid2, cid3]).await.unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap().as_ref(), data1);
        assert_eq!(results[1].as_ref().unwrap().as_ref(), data2);
        assert!(results[2].is_none());
    }

    #[tokio::test]
    async fn test_set_roots_marks_dirty() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = FileBlockStore::new(temp_file.path());

        assert!(!storage.is_dirty());

        storage.set_roots(vec![test_cid(1)]);
        assert!(storage.is_dirty());

        storage.flush().await.unwrap();
        assert!(!storage.is_dirty());
    }
}
