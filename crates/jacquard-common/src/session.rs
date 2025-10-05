//! Generic session storage traits and utilities.

use async_trait::async_trait;
use miette::Diagnostic;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::hash::Hash;
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
    /// Remove all stored sessions.
    async fn clear(&self) -> Result<(), SessionStoreError>;
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
    async fn clear(&self) -> Result<(), SessionStoreError> {
        self.0.write().await.clear();
        Ok(())
    }
}
