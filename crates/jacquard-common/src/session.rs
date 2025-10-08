//! Generic session storage traits and utilities.

use async_trait::async_trait;
use miette::Diagnostic;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::Display;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Errors emitted by session stores.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum SessionStoreError {
    /// Filesystem or I/O error
    #[error("I/O error: {0}")]
    #[diagnostic(code(jacquard::session_store::io))]
    Io(#[from] std::io::Error),
    /// Serialization error (e.g., JSON)
    #[error("serialization error: {0}")]
    #[diagnostic(code(jacquard::session_store::serde))]
    Serde(#[from] serde_json::Error),
    /// Any other error from a backend implementation
    #[error(transparent)]
    #[diagnostic(code(jacquard::session_store::other))]
    Other(#[from] Box<dyn StdError + Send + Sync>),
}

/// Pluggable storage for arbitrary session records.
#[async_trait]
pub trait SessionStore<K, T>: Send + Sync
where
    K: Eq + Hash,
    T: Clone,
{
    /// Get the current session if present.
    async fn get(&self, key: &K) -> Option<T>;
    /// Persist the given session.
    async fn set(&self, key: K, session: T) -> Result<(), SessionStoreError>;
    /// Delete the given session.
    async fn del(&self, key: &K) -> Result<(), SessionStoreError>;
}

/// In-memory session store suitable for short-lived sessions and tests.
#[derive(Clone)]
pub struct MemorySessionStore<K, T>(Arc<RwLock<HashMap<K, T>>>);

impl<K, T> Default for MemorySessionStore<K, T> {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }
}

#[async_trait]
impl<K, T> SessionStore<K, T> for MemorySessionStore<K, T>
where
    K: Eq + Hash + Send + Sync,
    T: Clone + Send + Sync + 'static,
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
#[derive(Clone, Debug)]
pub struct FileTokenStore {
    /// Path to the JSON file.
    pub path: PathBuf,
}

impl FileTokenStore {
    /// Create a new file token store at the given path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        std::fs::create_dir_all(path.as_ref().parent().unwrap()).unwrap();
        std::fs::write(path.as_ref(), b"{}").unwrap();

        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

#[async_trait::async_trait]
impl<
    K: Eq + Hash + Display + Send + Sync + 'static,
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
> SessionStore<K, T> for FileTokenStore
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
