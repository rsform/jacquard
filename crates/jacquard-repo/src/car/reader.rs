//! CAR file reading utilities
//!
//! Provides functions for reading CAR (Content Addressable aRchive) files into memory
//! or streaming them for large repositories.

use crate::error::{RepoError, Result};
use bytes::Bytes;
use cid::Cid as IpldCid;
use iroh_car::CarReader;
use n0_future::stream::{Stream, StreamExt};
use std::collections::BTreeMap;
use std::path::Path;
use std::pin::Pin;
use tokio::fs::File;

/// Parsed CAR file data
#[derive(Debug, Clone)]
pub struct ParsedCar {
    /// The first root CID from the CAR header
    pub root: IpldCid,
    /// All blocks in the CAR file
    pub blocks: BTreeMap<IpldCid, Bytes>,
}

/// Read entire CAR file into memory
///
/// Returns BTreeMap of CID -> block data (sorted order for determinism).
/// For large CAR files, consider using `stream_car()` instead.
pub async fn read_car(path: impl AsRef<Path>) -> Result<BTreeMap<IpldCid, Bytes>> {
    let file = File::open(path).await.map_err(|e| RepoError::io(e))?;

    let reader = CarReader::new(file).await.map_err(|e| RepoError::car(e))?;

    let mut blocks = BTreeMap::new();
    let stream = reader.stream();
    n0_future::pin!(stream);

    while let Some(result) = stream.next().await {
        let (cid, data) = result.map_err(|e| RepoError::car_parse(e))?;
        blocks.insert(cid, Bytes::from(data));
    }

    Ok(blocks)
}

/// Read CAR file header (roots only)
///
/// Useful for checking roots without loading all blocks.
pub async fn read_car_header(path: impl AsRef<Path>) -> Result<Vec<IpldCid>> {
    let file = File::open(path).await.map_err(|e| RepoError::io(e))?;

    let reader = CarReader::new(file).await.map_err(|e| RepoError::car(e))?;

    Ok(reader.header().roots().to_vec())
}

/// Parse CAR bytes into root and block map
///
/// For in-memory CAR data (e.g., from firehose commit messages, merkle proofs).
/// Returns the first root CID and all blocks.
pub async fn parse_car_bytes(data: &[u8]) -> Result<ParsedCar> {
    let reader = CarReader::new(data)
        .await
        .map_err(|e| RepoError::car_parse(e))?;

    let roots = reader.header().roots();
    let root = roots
        .first()
        .copied()
        .ok_or_else(|| RepoError::invalid("CAR file has no roots"))?;

    let mut blocks = BTreeMap::new();
    let stream = reader.stream();
    n0_future::pin!(stream);

    while let Some(result) = stream.next().await {
        let (cid, data) = result.map_err(|e| RepoError::car_parse(e))?;
        blocks.insert(cid, Bytes::from(data));
    }

    Ok(ParsedCar { root, blocks })
}

/// Stream CAR blocks without loading entire file into memory
///
/// Useful for processing large CAR files incrementally.
pub async fn stream_car(path: impl AsRef<Path>) -> Result<CarBlockStream> {
    let file = File::open(path).await.map_err(|e| RepoError::io(e))?;

    let reader = CarReader::new(file).await.map_err(|e| RepoError::car(e))?;

    let roots = reader.header().roots().to_vec();
    let stream = Box::pin(reader.stream());

    Ok(CarBlockStream { stream, roots })
}

/// Streaming CAR block reader
///
/// Iterates through CAR blocks without loading entire file into memory.
pub struct CarBlockStream {
    stream: Pin<
        Box<dyn Stream<Item = std::result::Result<(IpldCid, Vec<u8>), iroh_car::Error>> + Send>,
    >,
    roots: Vec<IpldCid>,
}

impl CarBlockStream {
    /// Get next block from the stream
    ///
    /// Returns `None` when stream is exhausted.
    pub async fn next(&mut self) -> Result<Option<(IpldCid, Bytes)>> {
        match self.stream.next().await {
            Some(result) => {
                let (cid, data) = result.map_err(|e| RepoError::car_parse(e))?;
                Ok(Some((cid, Bytes::from(data))))
            }
            None => Ok(None),
        }
    }

    /// Get the CAR file roots
    pub fn roots(&self) -> &[IpldCid] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use crate::DAG_CBOR_CID_CODEC;

    use super::*;
    use iroh_car::CarWriter;
    use jacquard_common::types::crypto::SHA2_256;
    use tempfile::NamedTempFile;
    use tokio::io::AsyncWriteExt;

    async fn make_test_car(roots: Vec<IpldCid>, blocks: Vec<(IpldCid, Vec<u8>)>) -> Vec<u8> {
        let mut buf = Vec::new();
        let header = iroh_car::CarHeader::new_v1(roots);
        let mut writer = CarWriter::new(header, &mut buf);

        for (cid, data) in blocks {
            writer.write(cid, data).await.unwrap();
        }

        writer.finish().await.unwrap();
        buf.flush().await.unwrap();
        buf
    }

    fn make_test_cid(value: u8) -> IpldCid {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&[value]);
        let mh = multihash::Multihash::wrap(SHA2_256, &hash).unwrap();
        IpldCid::new_v1(DAG_CBOR_CID_CODEC, mh) // dag-cbor codec
    }

    #[tokio::test]
    async fn test_parse_car_with_blocks() {
        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);
        let data1 = vec![1, 2, 3];
        let data2 = vec![4, 5, 6];

        let car_bytes = make_test_car(
            vec![cid1],
            vec![(cid1, data1.clone()), (cid2, data2.clone())],
        )
        .await;

        let parsed = parse_car_bytes(&car_bytes).await.unwrap();
        assert_eq!(parsed.root, cid1);
        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(parsed.blocks.get(&cid1).unwrap().as_ref(), &data1);
        assert_eq!(parsed.blocks.get(&cid2).unwrap().as_ref(), &data2);
    }

    #[tokio::test]
    async fn test_read_car_from_file() {
        let cid1 = make_test_cid(1);
        let data1 = vec![1, 2, 3];

        let car_bytes = make_test_car(vec![cid1], vec![(cid1, data1.clone())]).await;

        // Write to temp file
        let temp_file = NamedTempFile::new().unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut tokio::fs::File::from_std(temp_file.reopen().unwrap()),
            &car_bytes,
        )
        .await
        .unwrap();

        // Read back
        let blocks = read_car(temp_file.path()).await.unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks.get(&cid1).unwrap().as_ref(), &data1);
    }

    #[tokio::test]
    async fn test_read_car_header() {
        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);
        let data1 = vec![1, 2, 3];

        let car_bytes = make_test_car(vec![cid1, cid2], vec![(cid1, data1)]).await;

        let temp_file = NamedTempFile::new().unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut tokio::fs::File::from_std(temp_file.reopen().unwrap()),
            &car_bytes,
        )
        .await
        .unwrap();

        let roots = read_car_header(temp_file.path()).await.unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0], cid1);
        assert_eq!(roots[1], cid2);
    }

    #[tokio::test]
    async fn test_stream_car() {
        let cid1 = make_test_cid(1);
        let cid2 = make_test_cid(2);
        let data1 = vec![1, 2, 3];
        let data2 = vec![4, 5, 6];

        let car_bytes = make_test_car(
            vec![cid1],
            vec![(cid1, data1.clone()), (cid2, data2.clone())],
        )
        .await;

        let temp_file = NamedTempFile::new().unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut tokio::fs::File::from_std(temp_file.reopen().unwrap()),
            &car_bytes,
        )
        .await
        .unwrap();

        let mut stream = stream_car(temp_file.path()).await.unwrap();

        // Read first block
        let (cid, data) = stream.next().await.unwrap().unwrap();
        assert_eq!(cid, cid1);
        assert_eq!(data.as_ref(), &data1);

        // Read second block
        let (cid, data) = stream.next().await.unwrap().unwrap();
        assert_eq!(cid, cid2);
        assert_eq!(data.as_ref(), &data2);

        // Stream exhausted
        assert!(stream.next().await.unwrap().is_none());
    }
}
