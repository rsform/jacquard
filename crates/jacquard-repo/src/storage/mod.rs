//! Block storage abstraction for MST nodes and records

use crate::{error::Result, repo::CommitData};
use bytes::Bytes;
use cid::Cid as IpldCid;

/// Async block storage trait
///
/// Provides CID-keyed block storage for MST nodes, commits, and record data.
/// Implementations might use:
/// - In-memory HashMap ([`MemoryBlockStore`])
/// - CAR file ([`FileBlockStore`])
/// - SQLite/RocksDB (user-provided)
/// - Remote HTTP storage (user-provided)
///
/// Clone is required so MST can share storage references across tree operations.
///
/// # WASM Compatibility
///
/// The trait uses `trait_variant` to conditionally require `Send` only on non-WASM targets,
/// allowing it to work in browser environments where `Send` is not available.
///
/// # Example
///
/// ```rust,ignore
/// use jacquard_repo::storage::{BlockStore, MemoryBlockStore};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let storage = MemoryBlockStore::new();
///
/// // Store a block
/// let data = b"hello world";
/// let cid = storage.put(data).await?;
///
/// // Retrieve it
/// if let Some(retrieved) = storage.get(&cid).await? {
///     assert_eq!(retrieved, data);
/// }
/// # Ok(())
/// # }
/// ```
#[trait_variant::make(Send)]
pub trait BlockStore: Clone {
    /// Get a block by CID
    ///
    /// Returns `None` if the block is not found.
    async fn get(&self, cid: &IpldCid) -> Result<Option<Bytes>>;

    /// Put a block, return its CID
    ///
    /// The CID is calculated from the data using SHA-256 hash and DAG-CBOR codec.
    /// This ensures content addressing: the same data always produces the same CID.
    async fn put(&self, data: &[u8]) -> Result<IpldCid>;

    /// Check if a block exists without retrieving it
    ///
    /// This can be more efficient than `get()` for implementations that can check
    /// existence without reading the full block data.
    async fn has(&self, cid: &IpldCid) -> Result<bool>;

    /// Put many blocks at once (optimization for batch writes)
    ///
    /// Implementations should optimize this for batch operations where possible
    /// (e.g., single transaction, bulk insert). A simple implementation can just
    /// call `put()` individually.
    ///
    /// # Note
    ///
    /// The provided CIDs should match the data, but implementations may choose to
    /// recalculate and validate them.
    async fn put_many(
        &self,
        blocks: impl IntoIterator<Item = (IpldCid, Bytes)> + Send,
    ) -> Result<()>;

    /// Get multiple blocks at once (optimization for batch reads)
    ///
    /// Implementations should optimize this for batch operations where possible.
    /// A simple implementation can just call `get()` individually.
    ///
    /// Returns a vec of the same length as the input, with `None` for missing blocks.
    async fn get_many(&self, cids: &[IpldCid]) -> Result<Vec<Option<Bytes>>>;

    /// Apply a commit (atomic write + delete)
    ///
    /// Performs validated commit operations on the underlying storage:
    /// - Persists all blocks from `commit.blocks`
    /// - Deletes blocks listed in `commit.deleted_cids` (garbage collection)
    ///
    /// This should be atomic where possible - either both operations succeed or both fail.
    /// For implementations that don't support atomic operations, writes should happen first,
    /// then deletes.
    async fn apply_commit(&self, commit: CommitData) -> Result<()>;
}

pub mod file;
pub mod layered;
pub mod memory;

pub use file::FileBlockStore;
pub use layered::LayeredBlockStore;
pub use memory::MemoryBlockStore;
