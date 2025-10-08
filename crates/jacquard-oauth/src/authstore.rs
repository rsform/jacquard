use std::sync::Arc;

use jacquard_common::{
    IntoStatic,
    session::{FileTokenStore, SessionStore, SessionStoreError},
    types::did::Did,
};
use smol_str::SmolStr;

use crate::session::{AuthRequestData, ClientSessionData};

#[async_trait::async_trait]
pub trait ClientAuthStore {
    async fn get_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData<'_>>, SessionStoreError>;

    async fn upsert_session(&self, session: ClientSessionData<'_>)
    -> Result<(), SessionStoreError>;

    async fn delete_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<(), SessionStoreError>;

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData<'_>>, SessionStoreError>;

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData<'_>,
    ) -> Result<(), SessionStoreError>;

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError>;
}

#[async_trait::async_trait]
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
