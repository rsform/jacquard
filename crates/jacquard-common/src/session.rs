//! Generic session storage traits and utilities.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::error::Error as StdError;
use core::fmt;
use core::future::Future;
use core::hash::Hash;
#[cfg(feature = "std")]
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol_str::SmolStr;

use crate::bos::{BosStr, DefaultStr};
use crate::types::{did::Did, handle::Handle};

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

/// Shared storage key for app-password and OAuth sessions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionKey {
    /// Account DID.
    pub did: Did,
    /// Store-local session identifier.
    pub session_id: SmolStr,
}

impl SessionKey {
    /// Create a new session key.
    pub fn new(did: Did, session_id: impl Into<SmolStr>) -> Self {
        Self {
            did,
            session_id: session_id.into(),
        }
    }

    /// Borrow the account DID.
    pub fn did(&self) -> Did<&str> {
        self.did.borrow()
    }

    /// Borrow the session identifier.
    pub fn session_id(&self) -> &str {
        self.session_id.as_str()
    }
}

impl fmt::Display for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.did, self.session_id)
    }
}

impl From<(Did, SmolStr)> for SessionKey {
    fn from((did, session_id): (Did, SmolStr)) -> Self {
        Self { did, session_id }
    }
}

impl From<SessionKey> for (Did, SmolStr) {
    fn from(key: SessionKey) -> Self {
        (key.did, key.session_id)
    }
}

impl SessionHint<DefaultStr> {
    /// Build a session hint that matches any session.
    pub fn any() -> Self {
        SessionHint::Any
    }

    /// Build a session hint that matches a specific key.
    pub fn key(key: SessionKey) -> Self {
        SessionHint::Key(key)
    }

    /// Build a session hint that matches a login identifier.
    pub fn identifier(identifier: DefaultStr) -> Self {
        SessionHint::Identifier(identifier)
    }

    /// Build a session hint that matches a DID.
    pub fn did(did: Did<DefaultStr>) -> Self {
        SessionHint::Did(did)
    }

    /// Build a session hint that matches a handle.
    pub fn handle(handle: Handle<DefaultStr>) -> Self {
        SessionHint::Handle(handle)
    }
}

impl<'a> SessionHint<&'a str> {
    /// Build a borrowed session hint from CLI/login input.
    ///
    /// DIDs and handles become addressable session hints. Other inputs are kept as login
    /// identifiers, which can start authentication but do not match resolver-free stores.
    pub fn from_input(input: &'a str) -> Self {
        if let Ok(did) = Did::new(input) {
            SessionHint::Did(did)
        } else if let Ok(handle) = Handle::new(input) {
            SessionHint::Handle(handle)
        } else {
            SessionHint::Identifier(input)
        }
    }

    /// Build a borrowed session hint from optional CLI/login input.
    ///
    /// Missing input means "resume any existing session".
    pub fn from_optional_input(input: Option<&'a str>) -> Self {
        match input {
            Some(input) => Self::from_input(input),
            None => SessionHint::Any,
        }
    }
}

/// Resolver-free hint for choosing a stored session.
///
/// Matching in `jacquard-common` is intentionally key-only and does not perform identity
/// resolution. [`SessionHint::Handle`] cannot be matched from [`SessionKey`] values alone and
/// returns no match in [`match_session_key`]; higher-level stores may add handle-aware matching
/// when they have typed records containing handle metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionHint<S: BosStr = DefaultStr> {
    /// Use any available session.
    Any,
    /// Use the first session for the given DID.
    Did(Did<S>),
    /// Use a session for the given handle, if a higher-level matcher can resolve it.
    Handle(Handle<S>),
    /// Use this exact key.
    Key(SessionKey),
    /// Login/start-auth identifier that is not necessarily session-addressable.
    ///
    /// Examples include an email address, explicit PDS/entryway URL, or
    /// application-specific login input. Default resolver-free selectors do not
    /// match this as an existing session.
    Identifier(S),
}

/// Match a session key using only resolver-free key data.
pub fn match_session_key<I, S>(hint: &SessionHint<S>, keys: I) -> Option<SessionKey>
where
    I: IntoIterator<Item = SessionKey>,
    S: BosStr,
{
    match hint {
        SessionHint::Any => keys.into_iter().next(),
        SessionHint::Did(did) => keys
            .into_iter()
            .find(|key| key.did.as_str() == did.as_ref()),
        SessionHint::Handle(_) | SessionHint::Identifier(_) => None,
        SessionHint::Key(target) => keys.into_iter().find(|key| key == target),
    }
}

/// Selects a session from a hint, optionally returning richer implementation-specific data.
///
/// This trait is intentionally separate from [`SessionStore`]. Simple implementations may select
/// by enumerating store keys and filtering, while database-backed or otherwise indexed
/// implementations can resolve [`SessionHint::Key`] or [`SessionHint::Did`] without a full scan.
/// Higher-level crates can also implement selectors that resolve [`SessionHint::Handle`] using an
/// identity resolver and return metadata such as cached endpoints alongside the selected key.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait SessionSelector<M>: Send + Sync {
    /// Error returned by this selector.
    type Error;

    /// Select a matching session, if one exists.
    fn select_session<S: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<S>,
    ) -> impl Future<Output = Result<Option<M>, Self::Error>>;
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
    /// List known session keys when the backend supports enumeration.
    fn list_keys(&self) -> impl Future<Output = Result<Vec<K>, SessionStoreError>>
    where
        K: Clone,
    {
        async { Ok(Vec::new()) }
    }
}

/// In-memory session store suitable for short-lived sessions and tests.
#[derive(Clone)]
pub struct MemorySessionStore<K, T>(Arc<RwLock<BTreeMap<K, T>>>);

impl<K, T> Default for MemorySessionStore<K, T> {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(BTreeMap::new())))
    }
}

impl<K, T> SessionStore<K, T> for MemorySessionStore<K, T>
where
    K: Eq + Hash + Send + Sync + Ord,
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

    async fn list_keys(&self) -> Result<Vec<K>, SessionStoreError>
    where
        K: Clone,
    {
        Ok(self.0.read().await.keys().cloned().collect())
    }
}

impl<T> SessionSelector<SessionKey> for MemorySessionStore<SessionKey, T>
where
    T: Clone + Send + Sync,
{
    type Error = SessionStoreError;

    async fn select_session<S: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<S>,
    ) -> Result<Option<SessionKey>, Self::Error> {
        Ok(match_session_key(hint, self.list_keys().await?))
    }
}

/// File-backed token store using a JSON file.
///
/// NOT secure, only suitable for development.
///
/// Example
/// ```ignore
/// use jacquard::client::{AtClient, FileTokenStore};
/// let base = jacquard_common::deps::fluent_uri::Uri::parse("https://bsky.social").unwrap().to_owned();
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
impl FileTokenStore {
    /// Read a JSON value by string key.
    pub fn get_value(&self, key: &str) -> Result<Option<Value>, SessionStoreError> {
        let file = std::fs::read_to_string(&self.path)?;
        let store: Value = serde_json::from_str(&file)?;
        Ok(store.get(key).cloned())
    }

    /// Insert or replace a JSON value by string key.
    pub fn set_value(&self, key: impl Into<String>, value: Value) -> Result<(), SessionStoreError> {
        let file = std::fs::read_to_string(&self.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        if let Some(store) = store.as_object_mut() {
            store.insert(key.into(), value);
            std::fs::write(&self.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }

    /// Remove a JSON value by string key.
    pub fn remove_value(&self, key: &str) -> Result<(), SessionStoreError> {
        let file = std::fs::read_to_string(&self.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        if let Some(store) = store.as_object_mut() {
            store.remove(key);
            std::fs::write(&self.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }

    /// Return all JSON object entries in the store.
    pub fn entries(&self) -> Result<Vec<(String, Value)>, SessionStoreError> {
        let file = std::fs::read_to_string(&self.path)?;
        let store: Value = serde_json::from_str(&file)?;
        if let Some(store) = store.as_object() {
            Ok(store
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn session_key_display_uses_slash_separator() {
        let did = Did::new_static("did:plc:alice").unwrap();
        let key = SessionKey::new(did, "session_1");
        assert_eq!(key.to_string(), "did:plc:alice/session_1");
    }

    #[tokio::test]
    async fn memory_store_lists_keys() {
        let store = MemorySessionStore::<SessionKey, String>::default();
        let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "session");
        store.set(key.clone(), "value".to_string()).await.unwrap();
        assert_eq!(store.list_keys().await.unwrap(), vec![key]);
    }

    struct EmptyStore;

    impl SessionStore<SessionKey, String> for EmptyStore {
        async fn get(&self, _key: &SessionKey) -> Option<String> {
            None
        }

        async fn set(&self, _key: SessionKey, _session: String) -> Result<(), SessionStoreError> {
            Ok(())
        }

        async fn del(&self, _key: &SessionKey) -> Result<(), SessionStoreError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn default_list_keys_is_empty() {
        assert!(EmptyStore.list_keys().await.unwrap().is_empty());
    }

    #[test]
    fn match_session_key_is_resolver_free() {
        let alice = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "a");
        let bob = SessionKey::new(Did::new_static("did:plc:bob").unwrap(), "b");
        let keys = vec![alice.clone(), bob.clone()];

        assert_eq!(
            match_session_key(&SessionHint::any(), keys.clone()),
            Some(alice.clone())
        );
        assert_eq!(
            match_session_key(&SessionHint::Did(bob.did.clone()), keys.clone()),
            Some(bob.clone())
        );
        assert_eq!(
            match_session_key(&SessionHint::key(bob.clone()), keys.clone()),
            Some(bob.clone())
        );
        assert_eq!(
            match_session_key(
                &SessionHint::key(SessionKey::new(
                    Did::new_static("did:plc:carol").unwrap(),
                    "c",
                )),
                keys.clone(),
            ),
            None
        );
        assert_eq!(match_session_key(&SessionHint::any(), Vec::new()), None);
        assert_eq!(
            match_session_key(
                &SessionHint::<DefaultStr>::Handle(
                    Handle::new_static("alice.example.com").unwrap()
                ),
                keys.clone(),
            ),
            None
        );
        assert_eq!(
            match_session_key(
                &SessionHint::Identifier(SmolStr::new("alice@example.com")),
                keys
            ),
            None
        );
    }
}
