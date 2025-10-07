use jacquard_common::{session::SessionStoreError, types::did::Did};

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
