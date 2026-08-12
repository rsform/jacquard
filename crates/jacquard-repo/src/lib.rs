//! AT Protocol repository primitives
//!
//! This crate provides building blocks for working with AT Protocol repositories:
//!
//! - **MST (Merkle Search Tree)**: Immutable tree operations with deterministic structure
//! - **Commits**: Signed commit structures (versions 2 and 3) with signature verification
//! - **CAR I/O**: Import and export repositories in CAR (Content Addressable aRchive) format
//! - **Storage**: Pluggable block storage abstraction with in-memory and file-backed implementations
//!
//! # Design Philosophy
//!
//! - Core primitives are always available (MST, commits, storage)
//! - Optional high-level Repository API for convenience
//! - Immutable MST operations for referential transparency
//! - Zero-copy deserialization where possible
//! - Support for both current and future sync protocol versions
//!
//! Note: thank you very much to Rudy and Clinton, rsky was very helpful in figuring this all out.
//!
//! # Example
//!
//! ```rust,ignore
//! use jacquard_repo::{Mst, MemoryBlockStore};
//! use cid::Cid;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = MemoryBlockStore::new();
//! let mst = Mst::new(storage);
//!
//! // Add entries
//! let cid = /* ... */;
//! let new_mst = mst.add("app.bsky.feed.post/abc123", cid).await?;
//!
//! // Retrieve
//! if let Some(value) = new_mst.get("app.bsky.feed.post/abc123").await? {
//!     println!("Found: {}", value);
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod car;
pub mod commit;
pub mod error;
pub mod mst;
pub mod repo;
pub mod storage;
pub mod permissioned;

pub use error::{RepoError, RepoErrorKind, Result};
pub use mst::{Mst, MstDiff, WriteOp};
pub use repo::{CommitData, Repository};
pub use storage::{BlockStore, FileBlockStore, LayeredBlockStore, MemoryBlockStore};
pub use permissioned::{LtHash, PermissionedPath, SignedCommit};

/// DAG-CBOR codec identifier for CIDs (0x71)
pub const DAG_CBOR_CID_CODEC: u64 = 0x71;
