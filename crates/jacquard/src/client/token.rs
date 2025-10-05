use crate::client::AtpSession;
use async_trait::async_trait;
use jacquard_common::IntoStatic;
use jacquard_common::session::{SessionStore, SessionStoreError};
use jacquard_common::types::string::{Did, Handle};
use std::path::{Path, PathBuf};

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
impl SessionStore<Did<'static>, AtpSession> for FileTokenStore {
    async fn get(&self, key: &Did<'static>) -> Option<AtpSession> {
        let mut path = self.path.clone();
        path.push(key.to_string());
        let data = tokio::fs::read(&path).await.ok()?;
        let disk: FileSession = serde_json::from_slice(&data).ok()?;
        let did = Did::new_owned(disk.did).ok()?;
        let handle = Handle::new_owned(disk.handle).ok()?;
        Some(AtpSession {
            access_jwt: disk.access_jwt.into(),
            refresh_jwt: disk.refresh_jwt.into(),
            did: did.into_static(),
            handle: handle.into_static(),
        })
    }

    async fn set(&self, key: Did<'static>, session: AtpSession) -> Result<(), SessionStoreError> {
        let disk = FileSession {
            access_jwt: session.access_jwt.to_string(),
            refresh_jwt: session.refresh_jwt.to_string(),
            did: session.did.to_string(),
            handle: session.handle.to_string(),
        };
        let buf = serde_json::to_vec_pretty(&disk).map_err(SessionStoreError::from)?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SessionStoreError::from)?;
        }
        let mut path = self.path.clone();
        path.push(key.to_string());
        let tmp = path.with_extension("tmp");
        tokio::fs::write(&tmp, &buf)
            .await
            .map_err(SessionStoreError::from)?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(SessionStoreError::from)?;
        Ok(())
    }

    async fn del(&self, key: &Did<'static>) -> Result<(), SessionStoreError> {
        let mut path = self.path.clone();
        path.push(key.to_string());
        match tokio::fs::remove_file(&path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SessionStoreError::from(e)),
        }
    }

    async fn clear(&self) -> Result<(), SessionStoreError> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SessionStoreError::from(e)),
        }
    }
}
