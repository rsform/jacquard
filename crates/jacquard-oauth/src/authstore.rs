use std::future::Future;
use std::sync::Arc;

use dashmap::DashMap;
use jacquard_common::{
    IntoStatic,
    session::{SessionStore, SessionStoreError},
    types::did::Did,
};
use smol_str::{SmolStr, ToSmolStr, format_smolstr};

use crate::session::{AuthRequestData, ClientSessionData};

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait ClientAuthStore {
    fn get_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<ClientSessionData<'_>>, SessionStoreError>>;

    fn upsert_session(&self, session: ClientSessionData<'_>)
    -> impl Future<Output = Result<(), SessionStoreError>>;

    fn delete_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> impl Future<Output = Result<(), SessionStoreError>>;

    fn get_auth_req_info(
        &self,
        state: &str,
    ) -> impl Future<Output = Result<Option<AuthRequestData<'_>>, SessionStoreError>>;

    fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData<'_>,
    ) -> impl Future<Output = Result<(), SessionStoreError>>;

    fn delete_auth_req_info(&self, state: &str) -> impl Future<Output = Result<(), SessionStoreError>>;
}

pub struct MemoryAuthStore {
    sessions: DashMap<SmolStr, ClientSessionData<'static>>,
    auth_reqs: DashMap<SmolStr, AuthRequestData<'static>>,
}

impl MemoryAuthStore {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            auth_reqs: DashMap::new(),
        }
    }
}

impl ClientAuthStore for MemoryAuthStore {
    async fn get_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData<'_>>, SessionStoreError> {
        let key = format_smolstr!("{}_{}", did, session_id);
        Ok(self.sessions.get(&key).map(|v| v.clone()))
    }

    async fn upsert_session(
        &self,
        session: ClientSessionData<'_>,
    ) -> Result<(), SessionStoreError> {
        let key = format_smolstr!("{}_{}", session.account_did, session.session_id);
        self.sessions.insert(key, session.into_static());
        Ok(())
    }

    async fn delete_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        let key = format_smolstr!("{}_{}", did, session_id);
        self.sessions.remove(&key);
        Ok(())
    }

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData<'_>>, SessionStoreError> {
        Ok(self.auth_reqs.get(state).map(|v| v.clone()))
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData<'_>,
    ) -> Result<(), SessionStoreError> {
        self.auth_reqs.insert(
            auth_req_info.state.clone().to_smolstr(),
            auth_req_info.clone().into_static(),
        );
        Ok(())
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        self.auth_reqs.remove(state);
        Ok(())
    }
}

impl<T: ClientAuthStore + Send + Sync>
    SessionStore<(Did<'static>, SmolStr), ClientSessionData<'static>> for Arc<T>
{
    /// Get the current session if present.
    async fn get(&self, key: &(Did<'static>, SmolStr)) -> Option<ClientSessionData<'static>> {
        let (did, session_id) = key;
        self.as_ref()
            .get_session(did, session_id)
            .await
            .ok()
            .flatten()
            .into_static()
    }
    /// Persist the given session.
    async fn set(
        &self,
        _key: (Did<'static>, SmolStr),
        session: ClientSessionData<'static>,
    ) -> Result<(), SessionStoreError> {
        self.as_ref().upsert_session(session).await
    }
    /// Delete the given session.
    async fn del(&self, key: &(Did<'static>, SmolStr)) -> Result<(), SessionStoreError> {
        let (did, session_id) = key;
        self.as_ref().delete_session(did, session_id).await
    }
}
