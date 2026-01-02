//! Generic session storage traits and utilities.

use alloc::boxed::Box;
#[cfg(feature = "std")]
use alloc::string::ToString;
use alloc::sync::Arc;
use core::error::Error as StdError;
#[cfg(feature = "std")]
use core::fmt::Display;
use core::future::Future;
use core::hash::Hash;
use hashbrown::HashMap;
#[cfg(feature = "std")]
use miette::Diagnostic;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

// Use tokio's RwLock with std, maitake-sync's async RwLock for no_std
#[cfg(not(feature = "std"))]
use maitake_sync::RwLock;
#[cfg(feature = "std")]
use tokio::sync::RwLock;

/// Errors emitted by session stores.
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "std", derive(Diagnostic))]
#[non_exhaustive]
pub enum SessionStoreError {
    /// Filesystem or I/O error
    #[cfg(feature = "std")]
    #[error("I/O error: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::session_store::io)))]
    Io(#[from] std::io::Error),
    /// Serialization error (e.g., JSON)
    #[error("serialization error: {0}")]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::session_store::serde)))]
    Serde(#[from] serde_json::Error),
    /// Any other error from a backend implementation
    #[error(transparent)]
    #[cfg_attr(feature = "std", diagnostic(code(jacquard::session_store::other)))]
    Other(#[from] Box<dyn StdError + Send + Sync>),
}

/// Pluggable storage for arbitrary session records.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait SessionStore<K, T>: Send + Sync
where
    K: Eq + Hash,
    T: Clone,
{
    /// Get the current session if present.
    fn get(&self, key: &K) -> impl Future<Output = Option<T>>;
    /// Persist the given session.
    fn set(&self, key: K, session: T) -> impl Future<Output = Result<(), SessionStoreError>>;
    /// Delete the given session.
    fn del(&self, key: &K) -> impl Future<Output = Result<(), SessionStoreError>>;
}

/// In-memory session store suitable for short-lived sessions and tests.
#[derive(Clone)]
pub struct MemorySessionStore<K, T>(Arc<RwLock<HashMap<K, T>>>);

impl<K, T> Default for MemorySessionStore<K, T> {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }
}

impl<K, T> SessionStore<K, T> for MemorySessionStore<K, T>
where
    K: Eq + Hash + Send + Sync,
    T: Clone + Send + Sync,
{
    async fn get(&self, key: &K) -> Option<T> {
        self.0.read().await.get(key).cloned()
    }
    async fn set(&self, key: K, session: T) -> Result<(), SessionStoreError> {
        self.0.write().await.insert(key, session);
        Ok(())
    }
    async fn del(&self, key: &K) -> Result<(), SessionStoreError> {
        self.0.write().await.remove(key);
        Ok(())
    }
}

/// File-backed token store using a JSON file.
///
/// NOT secure, only suitable for development.
///
/// Example
/// ```ignore
/// use jacquard::client::{AtClient, FileTokenStore};
/// let base = url::Url::parse("https://bsky.social").unwrap();
/// let store = FileTokenStore::new("/tmp/jacquard-session.json");
/// let client = AtClient::new(reqwest::Client::new(), base, store);
/// ```
#[cfg(feature = "std")]
#[derive(Clone, Debug)]
pub struct FileTokenStore {
    /// Path to the JSON file.
    pub path: PathBuf,
}

#[cfg(feature = "std")]
impl FileTokenStore {
    /// Create a new file token store at the given path.
    ///
    /// Creates parent directories and initializes an empty JSON object if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Parent directories cannot be created
    /// - The file cannot be written
    pub fn try_new(path: impl AsRef<Path>) -> Result<Self, SessionStoreError> {
        let path = path.as_ref();

        // Create parent directories if they exist and don't already exist
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Initialize empty JSON object if file doesn't exist
        if !path.exists() {
            std::fs::write(path, b"{}")?;
        }

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Create a new file token store at the given path.
    ///
    /// # Panics
    ///
    /// Panics if parent directories cannot be created or the file cannot be written.
    /// Prefer [`try_new`](Self::try_new) for fallible construction.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self::try_new(path).expect("failed to initialize FileTokenStore")
    }
}

#[cfg(feature = "std")]
impl<K: Eq + Hash + Display + Send + Sync, T: Clone + Serialize + DeserializeOwned + Send + Sync>
    SessionStore<K, T> for FileTokenStore
{
    /// Get the current session if present.
    async fn get(&self, key: &K) -> Option<T> {
        let file = std::fs::read_to_string(&self.path).ok()?;
        let store: Value = serde_json::from_str(&file).ok()?;

        let session = store.get(key.to_string())?;
        serde_json::from_value(session.clone()).ok()
    }
    /// Persist the given session.
    async fn set(&self, key: K, session: T) -> Result<(), SessionStoreError> {
        let file = std::fs::read_to_string(&self.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        let key_string = key.to_string();
        if let Some(store) = store.as_object_mut() {
            store.insert(key_string, serde_json::to_value(session.clone())?);

            std::fs::write(&self.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }
    /// Delete the given session.
    async fn del(&self, key: &K) -> Result<(), SessionStoreError> {
        let file = std::fs::read_to_string(&self.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        let key_string = key.to_string();
        if let Some(store) = store.as_object_mut() {
            store.remove(&key_string);

            std::fs::write(&self.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }
}
