//! Session implementation for front-end clients that proxy to a dedicated backend
//!

//#[cfg(target_arch = "wasm32")]
use crate::client::SessionStoreError;
//#[cfg(target_arch = "wasm32")]
use crate::client::{AtpSession, credential_session::SessionKey};
//#[cfg(target_arch = "wasm32")]
use gloo_storage::{LocalStorage, SessionStorage, Storage};
//#[cfg(target_arch = "wasm32")]
use jacquard_common::{session::SessionStore, types::string::Did};
#[cfg(target_arch = "wasm32")]
use jacquard_oauth::authstore::ClientAuthStore;
#[cfg(target_arch = "wasm32")]
use jacquard_oauth::session::{AuthRequestData, ClientSessionData};
//#[cfg(target_arch = "wasm32")]
use std::future::Future;

//#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct BrowserAuthStore;

//#[cfg(target_arch = "wasm32")]
impl BrowserAuthStore {
    pub fn new() -> Self {
        Self
    }

    fn session_key(did: &Did<'_>, session_id: &str) -> String {
        format!("session_{}_{}", did.as_ref(), session_id)
    }

    fn auth_req_key(state: &str) -> String {
        format!("auth_req_{}", state)
    }
}

#[cfg(target_arch = "wasm32")]
impl ClientAuthStore for BrowserAuthStore {
    fn get_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<ClientSessionData<'_>>, SessionStoreError>> {
        let key = Self::session_key(did, session_id);
        async move {
            match LocalStorage::get::<serde_json::Value>(&key) {
                Ok(value) => {
                    let data: ClientSessionData<'static> =
                        jacquard::from_json_value::<ClientSessionData>(value).map_err(|e| {
                            SessionStoreError::Other(format!("Deserialize error: {}", e).into())
                        })?;
                    Ok(Some(data))
                }
                Err(gloo_storage::errors::StorageError::KeyNotFound(_)) => Ok(None),
                Err(e) => Err(SessionStoreError::Other(
                    format!("LocalStorage error: {}", e).into(),
                )),
            }
        }
    }

    fn upsert_session(
        &self,
        session: ClientSessionData<'_>,
    ) -> impl Future<Output = Result<(), SessionStoreError>> {
        async move {
            use jacquard::IntoStatic;

            let key = Self::session_key(&session.account_did, &session.session_id);
            let static_session = session.into_static();

            let value = serde_json::to_value(&static_session)
                .map_err(|e| SessionStoreError::Other(format!("Serialize error: {}", e).into()))?;

            LocalStorage::set(&key, &value).map_err(|e| {
                SessionStoreError::Other(format!("LocalStorage error: {}", e).into())
            })?;

            Ok(())
        }
    }

    fn delete_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> impl Future<Output = Result<(), SessionStoreError>> {
        let key = Self::session_key(did, session_id);
        async move {
            LocalStorage::delete(&key);
            Ok(())
        }
    }

    fn get_auth_req_info(
        &self,
        state: &str,
    ) -> impl Future<Output = Result<Option<AuthRequestData<'_>>, SessionStoreError>> {
        let key = Self::auth_req_key(state);
        async move {
            match LocalStorage::get::<serde_json::Value>(&key) {
                Ok(value) => {
                    let data: AuthRequestData<'static> =
                        jacquard::from_json_value::<AuthRequestData>(value).map_err(|e| {
                            SessionStoreError::Other(format!("Deserialize error: {}", e).into())
                        })?;
                    Ok(Some(data))
                }
                Err(gloo_storage::errors::StorageError::KeyNotFound(_)) => Ok(None),
                Err(e) => Err(SessionStoreError::Other(
                    format!("SessionStorage error: {}", e).into(),
                )),
            }
        }
    }

    fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData<'_>,
    ) -> impl Future<Output = Result<(), SessionStoreError>> {
        async move {
            use jacquard::IntoStatic;

            let key = Self::auth_req_key(&auth_req_info.state);
            let static_info = auth_req_info.clone().into_static();

            let value = serde_json::to_value(&static_info)
                .map_err(|e| SessionStoreError::Other(format!("Serialize error: {}", e).into()))?;

            LocalStorage::set(&key, &value).map_err(|e| {
                SessionStoreError::Other(format!("SessionStorage error: {}", e).into())
            })?;

            Ok(())
        }
    }

    fn delete_auth_req_info(
        &self,
        state: &str,
    ) -> impl Future<Output = Result<(), SessionStoreError>> {
        let key = Self::auth_req_key(state);
        async move {
            LocalStorage::delete(&key);
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl SessionStore<SessionKey, AtpSession> for BrowserAuthStore {
    fn get(&self, key: &SessionKey) -> impl Future<Output = Option<AtpSession>> + Send {
        let key = Self::session_key(&key.0, &key.1);
        async move {
            match LocalStorage::get::<serde_json::Value>(&key) {
                Ok(value) => {
                    let data: AtpSession = crate::from_json_value::<AtpSession>(value).ok()?;
                    Some(data)
                }
                Err(gloo_storage::errors::StorageError::KeyNotFound(_)) => None,
                Err(_) => None,
            }
        }
    }

    fn set(
        &self,
        key: SessionKey,
        session: AtpSession,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send {
        async move {
            let key = Self::session_key(&key.0, &key.1);

            let value = serde_json::to_value(&session)
                .map_err(|e| SessionStoreError::Other(format!("Serialize error: {}", e).into()))?;

            LocalStorage::set(&key, &value).map_err(|e| {
                SessionStoreError::Other(format!("LocalStorage error: {}", e).into())
            })?;

            Ok(())
        }
    }

    fn del(&self, key: &SessionKey) -> impl Future<Output = Result<(), SessionStoreError>> + Send {
        let key = Self::session_key(&key.0, &key.1);
        async move {
            LocalStorage::delete(&key);
            Ok(())
        }
    }
}

/// This might seem a little silly, and it sort of is, but the intended use here is for proxying requests to another client
#[cfg(target_arch = "wasm32")]
impl SessionStore<SessionKey, SessionKey> for BrowserAuthStore {
    fn get(&self, key: &SessionKey) -> impl Future<Output = Option<SessionKey>> + Send {
        let key = Self::session_key(&key.0, &key.1);
        async move {
            match LocalStorage::get::<serde_json::Value>(&key) {
                Ok(value) => {
                    let data: SessionKey = crate::from_json_value::<SessionKey>(value).ok()?;
                    Some(data)
                }
                Err(gloo_storage::errors::StorageError::KeyNotFound(_)) => None,
                Err(_) => None,
            }
        }
    }

    fn set(
        &self,
        key: SessionKey,
        session: SessionKey,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send {
        async move {
            let key = Self::session_key(&key.0, &key.1);

            let value = serde_json::to_value(&session)
                .map_err(|e| SessionStoreError::Other(format!("Serialize error: {}", e).into()))?;

            LocalStorage::set(&key, &value).map_err(|e| {
                SessionStoreError::Other(format!("LocalStorage error: {}", e).into())
            })?;

            Ok(())
        }
    }

    fn del(&self, key: &SessionKey) -> impl Future<Output = Result<(), SessionStoreError>> + Send {
        let key = Self::session_key(&key.0, &key.1);
        async move {
            LocalStorage::delete(&key);
            Ok(())
        }
    }
}
