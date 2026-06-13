use jacquard_common::IntoStatic;
use jacquard_common::bos::BosStr;
use jacquard_common::session::{
    FileTokenStore, SessionHint, SessionKey, SessionSelector, SessionStore, SessionStoreError,
};
use jacquard_common::types::string::Did;
use jacquard_oauth::session::{AuthRequestData, ClientSessionData};
use smol_str::{SmolStr, format_smolstr};

/// On-disk session records for app-password and OAuth flows, sharing a single JSON map.
///
/// The OAuth variants store [`ClientSessionData`] and [`AuthRequestData`] directly, rather
/// than through a separate serialization shim. With the default `SmolStr` backing these types
/// satisfy `DeserializeOwned`, so they can be (de)serialized as-is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StoredSession {
    /// App-password session.
    Atp(StoredAtSession),
    /// OAuth client session.
    ClientSession(ClientSessionData),
    /// OAuth authorization request state.
    AuthRequest(AuthRequestData),
}

/// Persisted representation of an app-password session plus its store-local session id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredAtSession {
    /// Session id label (e.g., "session")
    pub session_id: SmolStr,
    /// Stored app-password session.
    pub session: crate::client::AtpSession,
}

/// Convenience wrapper over `FileTokenStore` offering unified storage across auth modes.
pub struct FileAuthStore(FileTokenStore);

impl FileAuthStore {
    /// Create a new file-backed auth store wrapping `FileTokenStore`.
    ///
    /// # Errors
    ///
    /// Returns an error if parent directories cannot be created or the file cannot be written.
    pub fn try_new(path: impl AsRef<std::path::Path>) -> Result<Self, SessionStoreError> {
        Ok(Self(FileTokenStore::try_new(path)?))
    }

    /// Create a new file-backed auth store wrapping `FileTokenStore`.
    ///
    /// # Panics
    ///
    /// Panics if parent directories cannot be created or the file cannot be written.
    /// Prefer [`try_new`](Self::try_new) for fallible construction.
    pub fn new(path: impl AsRef<std::path::Path>) -> Self {
        Self(FileTokenStore::new(path))
    }

    fn atp_key(key: &SessionKey) -> SmolStr {
        format_smolstr!("atp:{}", key)
    }

    fn oauth_key(key: &SessionKey) -> SmolStr {
        format_smolstr!("oauth:{}", key)
    }

    fn oauth_state_key(state: &str) -> SmolStr {
        format_smolstr!("oauth-state:{}", state)
    }
}

impl jacquard_oauth::authstore::ClientAuthStore for FileAuthStore {
    async fn get_session<D: jacquard_common::bos::BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData>, SessionStoreError> {
        let key = SessionKey::new(did.borrow().into_static(), session_id);
        let Some(value) = self.0.get_value(&Self::oauth_key(&key))? else {
            return Ok(None);
        };
        if let StoredSession::ClientSession(session) = serde_json::from_value(value)? {
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        let key = SessionKey::new(session.account_did.clone(), session.session_id.clone());
        self.0.set_value(
            Self::oauth_key(&key),
            serde_json::to_value(StoredSession::ClientSession(session))?,
        )?;
        Ok(())
    }

    async fn delete_session<D: jacquard_common::bos::BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        let key = SessionKey::new(did.borrow().into_static(), session_id);
        self.0.remove_value(&Self::oauth_key(&key))
    }

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData>, SessionStoreError> {
        let key = Self::oauth_state_key(state);
        let Some(value) = self.0.get_value(&key)? else {
            return Ok(None);
        };
        if let StoredSession::AuthRequest(auth_req) = serde_json::from_value(value)? {
            Ok(Some(auth_req))
        } else {
            Ok(None)
        }
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        let key = Self::oauth_state_key(auth_req_info.state.as_ref());
        self.0.set_value(
            key,
            serde_json::to_value(StoredSession::AuthRequest(auth_req_info.clone()))?,
        )?;
        Ok(())
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        let key = Self::oauth_state_key(state);
        self.0.remove_value(&key)
    }

    async fn list_session_keys(&self) -> Result<Vec<SessionKey>, SessionStoreError> {
        let mut keys = Vec::new();
        for (_key, value) in self.0.entries()? {
            if let Ok(StoredSession::ClientSession(session)) =
                serde_json::from_value::<StoredSession>(value)
            {
                keys.push(SessionKey::new(session.account_did, session.session_id));
            }
        }
        Ok(keys)
    }
}

impl SessionStore<SessionKey, crate::client::AtpSession> for FileAuthStore {
    async fn get(&self, key: &SessionKey) -> Option<crate::client::AtpSession> {
        let value = self.0.get_value(&Self::atp_key(key)).ok()??;
        if let Ok(StoredSession::Atp(stored)) = serde_json::from_value::<StoredSession>(value) {
            Some(stored.session)
        } else {
            None
        }
    }

    async fn set(
        &self,
        key: SessionKey,
        session: crate::client::AtpSession,
    ) -> Result<(), jacquard_common::session::SessionStoreError> {
        let stored = StoredAtSession {
            session_id: key.session_id.clone(),
            session,
        };
        self.0.set_value(
            Self::atp_key(&key),
            serde_json::to_value(StoredSession::Atp(stored))?,
        )
    }

    async fn del(
        &self,
        key: &SessionKey,
    ) -> Result<(), jacquard_common::session::SessionStoreError> {
        self.0.remove_value(&Self::atp_key(key))
    }

    async fn list_keys(&self) -> Result<Vec<SessionKey>, SessionStoreError> {
        let mut keys = Vec::new();
        for (_key, value) in self.0.entries()? {
            if let Ok(StoredSession::Atp(session)) = serde_json::from_value::<StoredSession>(value)
            {
                keys.push(SessionKey::new(
                    session.session.did.clone(),
                    session.session_id,
                ));
            }
        }
        Ok(keys)
    }
}

impl SessionSelector<crate::client::credential_session::CredentialSessionMatch> for FileAuthStore {
    type Error = jacquard_common::error::ClientError;

    async fn select_session<Str: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<Str>,
    ) -> Result<Option<crate::client::credential_session::CredentialSessionMatch>, Self::Error>
    {
        match hint {
            SessionHint::Any => {
                let Some(key) = SessionStore::list_keys(self).await?.into_iter().next() else {
                    return Ok(None);
                };
                Ok(SessionStore::get(self, &key).await.map(|session| {
                    crate::client::credential_session::CredentialSessionMatch { key, session }
                }))
            }
            SessionHint::Did(did) => {
                for key in SessionStore::list_keys(self).await? {
                    if key.did.as_str() == did.as_ref() {
                        if let Some(session) = SessionStore::get(self, &key).await {
                            return Ok(Some(
                                crate::client::credential_session::CredentialSessionMatch {
                                    key,
                                    session,
                                },
                            ));
                        }
                    }
                }
                Ok(None)
            }
            SessionHint::Handle(handle) => {
                for key in SessionStore::list_keys(self).await? {
                    if let Some(session) = SessionStore::get(self, &key).await {
                        if session.handle.as_str() == handle.as_ref() {
                            return Ok(Some(
                                crate::client::credential_session::CredentialSessionMatch {
                                    key,
                                    session,
                                },
                            ));
                        }
                    }
                }
                Ok(None)
            }
            SessionHint::Key(key) => Ok(SessionStore::get(self, key).await.map(|session| {
                crate::client::credential_session::CredentialSessionMatch {
                    key: key.clone(),
                    session,
                }
            })),
            SessionHint::Identifier(_) => Ok(None),
        }
    }
}

impl SessionSelector<jacquard_oauth::authstore::OAuthSessionMatch> for FileAuthStore {
    type Error = SessionStoreError;

    async fn select_session<Str: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<Str>,
    ) -> Result<Option<jacquard_oauth::authstore::OAuthSessionMatch>, Self::Error> {
        match hint {
            SessionHint::Any => {
                let Some(key) = jacquard_oauth::authstore::ClientAuthStore::list_session_keys(self)
                    .await?
                    .into_iter()
                    .next()
                else {
                    return Ok(None);
                };
                oauth_match_for_key_file(self, key).await
            }
            SessionHint::Did(did) => {
                for key in
                    jacquard_oauth::authstore::ClientAuthStore::list_session_keys(self).await?
                {
                    if key.did.as_str() == did.as_ref() {
                        if let Some(matched) = oauth_match_for_key_file(self, key).await? {
                            return Ok(Some(matched));
                        }
                    }
                }
                Ok(None)
            }
            SessionHint::Handle(_) | SessionHint::Identifier(_) => Ok(None),
            SessionHint::Key(key) => oauth_match_for_key_file(self, key.clone()).await,
        }
    }
}

async fn oauth_match_for_key_file(
    store: &FileAuthStore,
    key: SessionKey,
) -> Result<Option<jacquard_oauth::authstore::OAuthSessionMatch>, SessionStoreError> {
    Ok(jacquard_oauth::authstore::ClientAuthStore::get_session(
        store,
        &key.did,
        key.session_id.as_str(),
    )
    .await?
    .map(|session| jacquard_oauth::authstore::OAuthSessionMatch { key, session }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::AtpSession;
    use crate::client::credential_session::SessionKey;
    use jacquard_common::deps::fluent_uri::Uri;
    use jacquard_common::types::string::{Did, Handle};
    use jacquard_oauth::scopes::Scopes;
    use jacquard_oauth::session::DpopClientData;
    use jacquard_oauth::types::OAuthTokenType;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("jacquard-test-{}-{nanos}.json", std::process::id()));
        p
    }

    fn oauth_session(did: &'static str, session_id: &'static str) -> ClientSessionData {
        let account_did = Did::new_static(did).unwrap();
        ClientSessionData {
            account_did: account_did.clone(),
            session_id: SmolStr::new_static(session_id),
            host_url: Uri::parse("https://pds.example.com").unwrap().to_owned(),
            authserver_url: SmolStr::new_static("https://issuer.example.com"),
            authserver_token_endpoint: SmolStr::new_static("https://issuer.example.com/token"),
            authserver_revocation_endpoint: None,
            scopes: Scopes::empty(),
            dpop_data: DpopClientData {
                dpop_key: jacquard_oauth::utils::generate_key(&[SmolStr::new_static("ES256")])
                    .unwrap(),
                dpop_authserver_nonce: SmolStr::default(),
                dpop_host_nonce: SmolStr::default(),
            },
            token_set: jacquard_oauth::types::TokenSet {
                iss: SmolStr::new_static("https://issuer.example.com"),
                sub: account_did,
                aud: SmolStr::new_static("https://pds.example.com"),
                scope: None,
                refresh_token: None,
                access_token: SmolStr::new_static("access"),
                token_type: OAuthTokenType::DPoP,
                expires_at: None,
            },
            resolved_scopes: None,
        }
    }

    #[tokio::test]
    async fn file_auth_store_roundtrip_atp() {
        let path = temp_file();
        // initialize empty store file
        fs::write(&path, "{}").unwrap();
        let store = FileAuthStore::new(&path);
        let session = AtpSession {
            access_jwt: "a".into(),
            refresh_jwt: "r".into(),
            did: Did::new_static("did:plc:alice").unwrap(),
            handle: Handle::new_static("alice.bsky.social").unwrap(),
            pds: None,
        };
        let key = SessionKey::new(session.did.clone(), "session");
        jacquard_common::session::SessionStore::set(&store, key.clone(), session.clone())
            .await
            .unwrap();
        let restored = jacquard_common::session::SessionStore::get(&store, &key)
            .await
            .unwrap();
        assert_eq!(restored.access_jwt.as_str(), "a");
        // clean up
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn file_auth_store_lists_only_atp_keys() {
        let path = temp_file();
        fs::write(&path, "{}").unwrap();
        let store = FileAuthStore::new(&path);
        let atp = AtpSession {
            access_jwt: "a".into(),
            refresh_jwt: "r".into(),
            did: Did::new_static("did:plc:alice").unwrap(),
            handle: Handle::new_static("alice.bsky.social").unwrap(),
            pds: None,
        };
        let atp_key = SessionKey::new(atp.did.clone(), "session");
        SessionStore::set(&store, atp_key.clone(), atp)
            .await
            .unwrap();
        jacquard_oauth::authstore::ClientAuthStore::upsert_session(
            &store,
            oauth_session("did:plc:bob", "oauth-session"),
        )
        .await
        .unwrap();

        assert_eq!(
            SessionStore::list_keys(&store).await.unwrap(),
            vec![atp_key]
        );
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn file_auth_store_lists_only_oauth_keys() {
        let path = temp_file();
        fs::write(&path, "{}").unwrap();
        let store = FileAuthStore::new(&path);
        let atp = AtpSession {
            access_jwt: "a".into(),
            refresh_jwt: "r".into(),
            did: Did::new_static("did:plc:alice").unwrap(),
            handle: Handle::new_static("alice.bsky.social").unwrap(),
            pds: None,
        };
        SessionStore::set(&store, SessionKey::new(atp.did.clone(), "session"), atp)
            .await
            .unwrap();
        jacquard_oauth::authstore::ClientAuthStore::upsert_session(
            &store,
            oauth_session("did:plc:bob", "oauth-session"),
        )
        .await
        .unwrap();

        assert_eq!(
            jacquard_oauth::authstore::ClientAuthStore::list_session_keys(&store)
                .await
                .unwrap(),
            vec![SessionKey::new(
                Did::new_static("did:plc:bob").unwrap(),
                "oauth-session",
            )]
        );
        let _ = fs::remove_file(&path);
    }
}
