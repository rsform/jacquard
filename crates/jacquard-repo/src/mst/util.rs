//! MST utility functions

use super::node::{NodeData, NodeEntry, TreeEntry};
use crate::error::{MstError, Result};
use crate::storage::BlockStore;
use bytes::Bytes;
use cid::Cid as IpldCid;
use sha2::{Digest, Sha256};

/// Compute CID from raw bytes
///
/// Uses SHA-256 hash and DAG-CBOR codec. Assumes data is already DAG-CBOR encoded.
pub fn compute_cid(data: &[u8]) -> Result<IpldCid> {
    use jacquard_common::types::crypto::{DAG_CBOR, SHA2_256};

    // SHA-256 hash
    let mut sha = Sha256::new();
    sha.update(data);
    let hash = sha.finalize().to_vec();
    // Build multihash using wrap (matches rsky approach)
    let mh = multihash::Multihash::<64>::wrap(SHA2_256, hash.as_slice())
        .map_err(|e| MstError::InvalidNode(e.to_string()))?;

    // Build CID with DAG-CBOR codec
    Ok(IpldCid::new_v1(DAG_CBOR, mh))
}

/// Serialize node to DAG-CBOR and compute CID
///
/// Uses SHA-256 hash and DAG-CBOR codec.
pub fn node_to_cid(node: &NodeData) -> Result<IpldCid> {
    let cbor =
        serde_ipld_dagcbor::to_vec(node).map_err(|e| MstError::Serialization(Box::new(e)))?;
    compute_cid(&cbor)
}

/// Calculate layer (depth) for a key based on its hash
///
/// Per atproto spec: depth = floor(leading_zero_bits / 2)
/// This gives a fanout of 4 (counting 2-bit chunks of zeros).
/// More leading zeros = deeper layer.
pub fn layer_for_key(key: &str) -> usize {
    let hash = Sha256::digest(key.as_bytes());
    leading_zeros(&hash) / 2
}

/// Count leading zero bits in hash
fn leading_zeros(hash: &[u8]) -> usize {
    let mut count = 0;
    for byte in hash {
        if *byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros() as usize;
            break;
        }
    }
    count
}

/// Validate MST key format
///
/// Keys must match: [a-zA-Z0-9._:~-]+
/// Max length: 256 bytes (atproto limit)
pub fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(MstError::EmptyKey.into());
    }

    if key.len() > 256 {
        return Err(MstError::KeyTooLong {
            len: key.len(),
            max: 256,
        }
        .into());
    }

    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '~' | '-' | '/'))
    {
        return Err(MstError::InvalidKeyChars {
            key: key.to_string(),
        }
        .into());
    }

    Ok(())
}

/// Count shared prefix length between two strings
pub fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Serialize flat entries to wire format (with prefix compression)
///
/// Converts in-memory `Vec<NodeEntry>` to DAG-CBOR `NodeData`.
/// - First `Tree` entry → `left` pointer
/// - Each `Leaf` → entry with prefix compression
/// - `Tree` after `Leaf` → that leaf's `tree` pointer
pub fn serialize_node_data<'a, S: BlockStore + Sync + 'static>(
    entries: &'a [NodeEntry<S>],
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<NodeData>> + Send + 'a>> {
    Box::pin(async move {
        let mut data = NodeData {
            left: None,
            entries: Vec::new(),
        };

        let mut i = 0;

        // First entry if Tree → becomes left pointer
        if let Some(NodeEntry::Tree(tree)) = entries.get(0) {
            data.left = Some(tree.get_pointer().await?);
            i += 1;
        }

        // Process remaining entries
        let mut last_key = String::new();
        while i < entries.len() {
            let entry = &entries[i];

            if let NodeEntry::Leaf { key, value } = entry {
                i += 1;

                // Calculate prefix with last key
                let prefix_len = common_prefix_len(&last_key, key.as_str());
                let key_suffix = &key.as_str()[prefix_len..];

                // Check for Tree after this Leaf
                let tree_ptr = if let Some(NodeEntry::Tree(tree)) = entries.get(i) {
                    i += 1;
                    Some(tree.get_pointer().await?)
                } else {
                    None
                };

                data.entries.push(TreeEntry {
                    prefix_len: prefix_len.try_into().map_err(|_| {
                        MstError::InvalidNode(format!(
                            "Prefix length {} exceeds u8::MAX",
                            prefix_len
                        ))
                    })?,
                    key_suffix: Bytes::copy_from_slice(key_suffix.as_bytes()),
                    value: *value,
                    tree: tree_ptr,
                });

                last_key = key.as_str().to_string();
            } else {
                return Err(
                    MstError::InvalidNode("Two Trees adjacent in flat entries".into()).into(),
                );
            }
        }

        Ok(data)
    })
}

/// Deserialize wire format to flat entries
///
/// Converts DAG-CBOR `NodeData` to in-memory `Vec<NodeEntry>`.
/// - `left` pointer → prepend `Tree` entry
/// - Each entry → `Leaf` with reconstructed full key
/// - `tree` pointer → append `Tree` entry
pub fn deserialize_node_data<S: BlockStore + Sync + 'static>(
    storage: std::sync::Arc<S>,
    data: &NodeData,
    layer: Option<usize>,
) -> Result<Vec<NodeEntry<S>>> {
    use crate::mst::Mst;

    let mut entries = Vec::new();

    // Left pointer → prepend Tree
    if let Some(left_cid) = data.left {
        let child_layer = layer.map(|l| if l > 0 { l - 1 } else { 0 });
        entries.push(NodeEntry::Tree(Mst::load(
            storage.clone(),
            left_cid,
            child_layer,
        )));
    }

    // Process entries
    let mut last_key = String::new();
    for entry in &data.entries {
        // Reconstruct full key from prefix
        let key_str = std::str::from_utf8(&entry.key_suffix)
            .map_err(|e| MstError::InvalidNode(format!("Invalid UTF-8 in key suffix: {}", e)))?;
        let prefix_len = entry.prefix_len as usize;
        let full_key = format!("{}{}", &last_key[..prefix_len], key_str);

        // Append Leaf
        entries.push(NodeEntry::Leaf {
            key: smol_str::SmolStr::new(&full_key),
            value: entry.value,
        });

        last_key = full_key;

        // Tree pointer → append Tree
        if let Some(tree_cid) = entry.tree {
            let child_layer = layer.map(|l| if l > 0 { l - 1 } else { 0 });
            entries.push(NodeEntry::Tree(Mst::load(
                storage.clone(),
                tree_cid,
                child_layer,
            )));
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_key_valid() {
        assert!(validate_key("app.bsky.feed.post/abc123").is_ok());
        assert!(validate_key("foo.bar/test-key_2024").is_ok());
        assert!(validate_key("a").is_ok());
    }

    #[test]
    fn test_validate_key_empty() {
        assert!(validate_key("").is_err());
    }

    #[test]
    fn test_validate_key_too_long() {
        let long_key = "a".repeat(257);
        assert!(validate_key(&long_key).is_err());
    }

    #[test]
    fn test_validate_key_invalid_chars() {
        assert!(validate_key("key with spaces").is_err());
        assert!(validate_key("key@invalid").is_err());
        assert!(validate_key("key#hash").is_err());
    }

    #[test]
    fn test_common_prefix_len() {
        assert_eq!(common_prefix_len("hello", "help"), 3);
        assert_eq!(common_prefix_len("abc", "abc"), 3);
        assert_eq!(common_prefix_len("abc", "def"), 0);
        assert_eq!(common_prefix_len("", "test"), 0);
    }

    #[test]
    fn test_layer_for_key() {
        // Just ensure it returns a reasonable value
        let layer = layer_for_key("app.bsky.feed.post/test");
        assert!(layer < 256); // SHA-256 has 256 bits max

        // Same key should always give same layer
        let layer2 = layer_for_key("app.bsky.feed.post/test");
        assert_eq!(layer, layer2);
    }

    #[test]
    fn test_leading_zeros() {
        // [0, 0, 0, 1] = 8 + 8 + 8 + 7 = 31 leading zeros
        assert_eq!(leading_zeros(&[0, 0, 0, 1]), 31);
        // [0xFF, ...] = 0 leading zeros (first byte has leading 1s)
        assert_eq!(leading_zeros(&[0xFF, 0, 0]), 0);
        // [0, 0x80] = 8 + 0 = 8 leading zeros (0x80 = 0b10000000)
        assert_eq!(leading_zeros(&[0, 0x80]), 8);
        // [0, 0x01] = 8 + 7 = 15 leading zeros (0x01 = 0b00000001)
        assert_eq!(leading_zeros(&[0, 0x01]), 15);
    }
}
