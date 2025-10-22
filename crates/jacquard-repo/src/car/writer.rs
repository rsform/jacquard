//! CAR file writing utilities
//!
//! Provides functions for writing blocks to CAR (Content Addressable aRchive) files.

use crate::error::{RepoError, Result};
use crate::mst::tree::Mst;
use crate::storage::BlockStore;
use bytes::Bytes;
use cid::Cid as IpldCid;
use iroh_car::CarWriter;
use std::collections::BTreeMap;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

/// Write blocks to CAR file
///
/// Roots should contain commit CID(s).
/// Blocks are written in sorted CID order (BTreeMap) for determinism.
pub async fn write_car(
    path: impl AsRef<Path>,
    roots: Vec<IpldCid>,
    blocks: BTreeMap<IpldCid, Bytes>,
) -> Result<()> {
    let file = File::create(path).await.map_err(|e| RepoError::io(e))?;

    let header = iroh_car::CarHeader::new_v1(roots);
    let mut writer = CarWriter::new(header, file);

    for (cid, data) in blocks {
        writer
            .write(cid, data.as_ref())
            .await
            .map_err(|e| RepoError::car(e))?;
    }

    writer.finish().await.map_err(|e| RepoError::car(e))?;

    Ok(())
}

/// Write blocks to CAR bytes (in-memory)
///
/// Like `write_car()` but writes to a `Vec<u8>` instead of a file.
/// Useful for tests and proof generation.
pub async fn write_car_bytes(root: IpldCid, blocks: BTreeMap<IpldCid, Bytes>) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let header = iroh_car::CarHeader::new_v1(vec![root]);
    let mut writer = CarWriter::new(header, &mut buffer);

    for (cid, data) in blocks {
        writer
            .write(cid, data.as_ref())
            .await
            .map_err(|e| RepoError::car(e))?;
    }

    writer.finish().await.map_err(|e| RepoError::car(e))?;

    buffer.flush().await.map_err(|e| RepoError::io(e))?;

    Ok(buffer)
}

/// Write MST + commit to CAR file
///
/// Streams blocks directly to CAR file:
/// - Commit block (from storage)
/// - All MST node blocks (from storage)
/// - All record blocks (from storage)
///
/// Uses streaming to avoid loading all blocks into memory.
pub async fn export_repo_car<S: BlockStore + Sync + 'static>(
    path: impl AsRef<Path>,
    commit_cid: IpldCid,
    mst: &Mst<S>,
) -> Result<()> {
    let file = File::create(path).await.map_err(|e| RepoError::io(e))?;

    let header = iroh_car::CarHeader::new_v1(vec![commit_cid]);
    let mut writer = CarWriter::new(header, file);

    // Write commit block first
    let storage = mst.storage();
    let commit_data = storage
        .get(&commit_cid)
        .await?
        .ok_or_else(|| RepoError::not_found("commit", &commit_cid))?;

    writer
        .write(commit_cid, &commit_data)
        .await
        .map_err(|e| RepoError::car(e))?;

    // Stream MST and record blocks
    mst.write_blocks_to_car(&mut writer).await?;

    // Finish writing
    writer.finish().await.map_err(|e| RepoError::car(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DAG_CBOR_CID_CODEC;
    use crate::car::reader::read_car;
    use crate::mst::tree::Mst;
    use crate::storage::memory::MemoryBlockStore;
    use jacquard_common::types::crypto::SHA2_256;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn make_test_cid(value: u8) -> IpldCid {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&[value]);
        let mh = multihash::Multihash::wrap(SHA2_256, &hash).unwrap();

        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    #[tokio::test]
    async fn test_write_car_with_blocks() {
        let temp_file = NamedTempFile::new().unwrap();

        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);
        let data1 = Bytes::from_static(&[1, 2, 3]);
        let data2 = Bytes::from_static(&[4, 5, 6]);

        let mut blocks = BTreeMap::new();
        blocks.insert(cid1, data1.clone());
        blocks.insert(cid2, data2.clone());

        write_car(temp_file.path(), vec![cid1], blocks)
            .await
            .unwrap();

        // Read back and verify
        let read_blocks = read_car(temp_file.path()).await.unwrap();
        assert_eq!(read_blocks.len(), 2);
        assert_eq!(read_blocks.get(&cid1).unwrap(), &data1);
        assert_eq!(read_blocks.get(&cid2).unwrap(), &data2);
    }

    #[tokio::test]
    async fn test_export_mst_to_car() {
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        // Add some entries
        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);

        let mst = mst.add("app.bsky.feed.post/abc123", cid1).await.unwrap();
        let mst = mst.add("app.bsky.feed.post/def456", cid2).await.unwrap();

        // Persist MST blocks to storage
        mst.persist().await.unwrap();

        // Persist record blocks to storage
        storage
            .put_with_cid(cid1, Bytes::from_static(&[1, 1, 1]))
            .await
            .unwrap();
        storage
            .put_with_cid(cid2, Bytes::from_static(&[2, 2, 2]))
            .await
            .unwrap();

        // Create and persist commit block
        let commit_cid = make_test_cid(99);
        let commit_data = Bytes::from_static(&[99, 99, 99]);
        storage
            .put_with_cid(commit_cid, commit_data.clone())
            .await
            .unwrap();

        let temp_file = NamedTempFile::new().unwrap();

        // Export to CAR
        export_repo_car(temp_file.path(), commit_cid, &mst)
            .await
            .unwrap();

        // Read back and verify
        let blocks = read_car(temp_file.path()).await.unwrap();

        // Should have commit + MST nodes + record blocks
        assert!(blocks.contains_key(&commit_cid));
        assert_eq!(blocks.get(&commit_cid).unwrap(), &commit_data);

        // Should have at least the root node
        let root_cid = mst.root().await.unwrap();
        assert!(blocks.contains_key(&root_cid));

        // Should have record blocks
        assert!(blocks.contains_key(&cid1));
        assert!(blocks.contains_key(&cid2));
    }
}
