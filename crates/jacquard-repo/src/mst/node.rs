//! MST node data structures

use std::fmt;

use bytes::Bytes;
use cid::Cid as IpldCid;
use smol_str::SmolStr;

use crate::{mst::Mst, storage::BlockStore};

/// Entry in an MST node - either a subtree or a leaf
///
/// This is the in-memory representation used for tree operations.
/// MST operations work on flat `Vec<NodeEntry>` where entries are interleaved:
/// `[Tree, Leaf, Tree, Leaf, Leaf, Tree]` etc.
///
/// The wire format (CBOR) is different - see `NodeData` and `TreeEntry`.
#[derive(Clone)]
pub enum NodeEntry<S> {
    /// Subtree reference
    ///
    /// Will be lazily loaded from storage when needed.
    Tree(Mst<S>),

    /// Leaf node with key-value pair
    Leaf {
        /// Full key (not prefix-compressed in memory)
        key: SmolStr,
        /// CID of the record value
        value: IpldCid,
    },
}

impl<S: BlockStore> fmt::Debug for NodeEntry<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeEntry::Tree(t) => write!(f, "{:?}", t),
            NodeEntry::Leaf { key, value } => {
                write!(f, "Leaf {{ key: {}, value: {} }}", key, value)
            }
        }
    }
}

impl<S> NodeEntry<S> {
    /// Check if this is a tree entry
    pub fn is_tree(&self) -> bool {
        matches!(self, NodeEntry::Tree(_))
    }

    /// Check if this is a leaf entry
    pub fn is_leaf(&self) -> bool {
        matches!(self, NodeEntry::Leaf { .. })
    }

    /// Get the key if this is a leaf
    pub fn leaf_key(&self) -> Option<&str> {
        match self {
            NodeEntry::Leaf { key, .. } => Some(key.as_str()),
            NodeEntry::Tree(_) => None,
        }
    }
}

// ============================================================================
// Wire format structures (for CBOR serialization)
// ============================================================================
//
// These represent the on-disk/network format with prefix compression.
// Conversion functions will be in util.rs.

/// Wire format entry (prefix-compressed leaf with optional subtree pointer)
///
/// This is what gets serialized to CBOR. In memory, we use the flat
/// `Vec<NodeEntry>` representation instead.
///
/// **IMPORTANT:** Fields MUST be in alphabetical order (k, p, t, v) to match
/// DAG-CBOR canonical form. Even though serde_ipld_dagcbor should handle this,
/// we define them in order to be explicit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreeEntry {
    /// Key suffix after prefix (stored as bytes in wire format)
    ///
    /// Must use serde_bytes to serialize as CBOR byte string (major type 2)
    /// instead of array of integers (major type 4)
    #[serde(rename = "k", with = "crate::commit::serde_bytes_helper")]
    pub key_suffix: Bytes,

    /// Prefix length (shared chars with previous key in node)
    ///
    /// Must be u8 (not usize) to match CBOR encoding in reference implementations
    #[serde(rename = "p")]
    pub prefix_len: u8,

    /// Optional subtree pointer (CID of child MST node)
    ///
    /// Serializes as explicit `null` when None (AT Protocol spec requirement for determinism).
    #[serde(rename = "t")]
    pub tree: Option<IpldCid>,

    /// CID of the record value
    #[serde(rename = "v")]
    pub value: IpldCid,
}

/// Wire format node data (serialized as DAG-CBOR)
///
/// This is the structure that gets written to storage. The in-memory
/// representation uses `Vec<NodeEntry>` instead.
///
/// # Conversion rules
///
/// **Serialization (flat → wire):**
/// - First entry if `Tree` → becomes `left` pointer
/// - Each `Leaf` → becomes entry in `entries`
/// - `Tree` after `Leaf` → becomes that leaf's `tree` pointer
///
/// **Deserialization (wire → flat):**
/// - `left` if present → prepend `Tree` entry
/// - Each entry → append `Leaf`
/// - Each `tree` if present → append `Tree` entry
///
/// # Nullability requirement
///
/// **CRITICAL:** All `Option<T>` fields MUST serialize as explicit `null` (not skip).
/// This is an AT Protocol spec requirement for cross-implementation determinism.
/// Skipping vs explicit null produces different CBOR bytes → different CIDs → broken interop.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeData {
    /// Left-most subtree pointer
    ///
    /// Serializes as explicit `null` when None (AT Protocol spec requirement for determinism).
    #[serde(rename = "l")]
    pub left: Option<IpldCid>,

    /// Entries in this node (sorted by full key, prefix-compressed)
    #[serde(rename = "e")]
    pub entries: Vec<TreeEntry>,
}
