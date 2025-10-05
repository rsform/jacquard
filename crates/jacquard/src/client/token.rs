use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

use super::Session;
use jacquard_common::IntoStatic;
use jacquard_common::types::string::{Did, Handle};

/// Errors emitted by token stores.
#[derive(Debug, Error)]
pub enum TokenStoreError {
    /// An underlying I/O or serialization error with context.
    #[error("token store error: {0}")]
    Other(String),
}

/// Pluggable session token storage (memory, disk, browser, etc.).
#[async_trait]
pub trait TokenStore: Send + Sync {
    /// Get the current session if present.
    async fn get(&self) -> Option<Session>;
    /// Persist the given session.
    async fn set(&self, session: Session) -> Result<(), TokenStoreError>;
    /// Remove any stored session.
    async fn clear(&self) -> Result<(), TokenStoreError>;
}

/// In-memory token store suitable for short-lived sessions and tests.
#[derive(Default, Clone)]
pub struct MemoryTokenStore(Arc<tokio::sync::RwLock<Option<Session>>>);

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn get(&self) -> Option<Session> {
        self.0.read().await.clone()
    }
    async fn set(&self, session: Session) -> Result<(), TokenStoreError> {
        *self.0.write().await = Some(session);
        Ok(())
    }
    async fn clear(&self) -> Result<(), TokenStoreError> {
        *self.0.write().await = None;
        Ok(())
    }
}

/// File-backed token store using a JSON file.
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
    path: PathBuf,
}

impl FileTokenStore {
    /// Create a new file token store at the given path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FileSession {
    access_jwt: String,
    refresh_jwt: String,
    did: String,
    handle: String,
}

#[async_trait]
impl TokenStore for FileTokenStore {
    async fn get(&self) -> Option<Session> {
        let data = tokio::fs::read(&self.path).await.ok()?;
        let disk: FileSession = serde_json::from_slice(&data).ok()?;
        let did = Did::new_owned(disk.did).ok()?;
        let handle = Handle::new_owned(disk.handle).ok()?;
        Some(Session {
            access_jwt: disk.access_jwt.into(),
            refresh_jwt: disk.refresh_jwt.into(),
            did: did.into_static(),
            handle: handle.into_static(),
        })
    }

    async fn set(&self, session: Session) -> Result<(), TokenStoreError> {
        let disk = FileSession {
            access_jwt: session.access_jwt.to_string(),
            refresh_jwt: session.refresh_jwt.to_string(),
            did: session.did.to_string(),
            handle: session.handle.to_string(),
        };
        let buf =
            serde_json::to_vec_pretty(&disk).map_err(|e| TokenStoreError::Other(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| TokenStoreError::Other(e.to_string()))?;
        }
        let tmp = self.path.with_extension("tmp");
        tokio::fs::write(&tmp, &buf)
            .await
            .map_err(|e| TokenStoreError::Other(e.to_string()))?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|e| TokenStoreError::Other(e.to_string()))?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), TokenStoreError> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(TokenStoreError::Other(e.to_string())),
        }
    }
}
