use jacquard_common::IntoStatic;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::session::{
    FileTokenStore, SessionHint, SessionKey, SessionSelector, SessionStore, SessionStoreError,
};
use jacquard_common::types::string::{Datetime, Did};
use jacquard_oauth::scopes::Scopes;
use jacquard_oauth::session::{AuthRequestData, ClientSessionData, DpopClientData, DpopReqData};
use jacquard_oauth::types::OAuthTokenType;
use jose_jwk::Key;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// On-disk session records for app-password and OAuth flows, sharing a single JSON map.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StoredSession {
    /// App-password session
    Atp(StoredAtSession),
    /// OAuth client session
    OAuth(OAuthSession),
    /// OAuth authorization request state
    OAuthState(OAuthState),
}

/// Persisted representation of an app-password session plus its store-local session id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredAtSession {
    /// Session id label (e.g., "session")
    pub session_id: String,
    /// Stored app-password session.
    pub session: crate::client::AtpSession,
}

/// Persisted OAuth client session (on-disk format).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OAuthSession {
    /// Account DID
    account_did: String,
    /// Client-generated session id (usually auth `state`)
    session_id: String,

    /// Base URL of the resource server (PDS)
    host_url: Uri<String>,

    /// Base URL of the authorization server (PDS or entryway)
    authserver_url: String,

    /// Full token endpoint URL
    authserver_token_endpoint: String,

    /// Full revocation endpoint URL, if available
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    authserver_revocation_endpoint: Option<String>,

    /// Granted scopes (space-separated, normalized).
    scopes: String,

    /// Client DPoP key material
    pub dpop_key: Key,
    /// Current auth server DPoP nonce
    pub dpop_authserver_nonce: String,
    /// Current resource server (PDS) DPoP nonce
    pub dpop_host_nonce: String,

    /// Token response issuer
    pub iss: String,
    /// Token subject (DID)
    pub sub: String,
    /// Token audience (verified PDS URL)
    pub aud: String,
    /// Token scopes (raw) if provided
    pub scope: Option<String>,

    /// Refresh token
    pub refresh_token: Option<String>,
    /// Access token
    pub access_token: String,
    /// Token type (e.g., DPoP)
    pub token_type: OAuthTokenType,

    /// Expiration timestamp
    pub expires_at: Option<Datetime>,
}

impl<S: jacquard_common::bos::BosStr + Ord> From<ClientSessionData<S>> for OAuthSession {
    fn from(data: ClientSessionData<S>) -> Self {
        OAuthSession {
            account_did: AsRef::<str>::as_ref(&data.account_did).to_owned(),
            session_id: AsRef::<str>::as_ref(&data.session_id).to_owned(),
            host_url: data.host_url.clone(),
            authserver_url: AsRef::<str>::as_ref(&data.authserver_url).to_owned(),
            authserver_token_endpoint: AsRef::<str>::as_ref(&data.authserver_token_endpoint)
                .to_owned(),
            authserver_revocation_endpoint: data
                .authserver_revocation_endpoint
                .map(|s| AsRef::<str>::as_ref(&s).to_owned()),
            scopes: String::from(data.scopes.to_normalized_string()),
            dpop_key: data.dpop_data.dpop_key,
            dpop_authserver_nonce: AsRef::<str>::as_ref(&data.dpop_data.dpop_authserver_nonce)
                .to_owned(),
            dpop_host_nonce: AsRef::<str>::as_ref(&data.dpop_data.dpop_host_nonce).to_owned(),
            iss: AsRef::<str>::as_ref(&data.token_set.iss).to_owned(),
            sub: AsRef::<str>::as_ref(&data.token_set.sub).to_owned(),
            aud: AsRef::<str>::as_ref(&data.token_set.aud).to_owned(),
            scope: data
                .token_set
                .scope
                .map(|s| AsRef::<str>::as_ref(&s).to_owned()),
            refresh_token: data
                .token_set
                .refresh_token
                .map(|s| AsRef::<str>::as_ref(&s).to_owned()),
            access_token: AsRef::<str>::as_ref(&data.token_set.access_token).to_owned(),
            token_type: data.token_set.token_type,
            expires_at: data.token_set.expires_at,
        }
    }
}

impl From<OAuthSession> for ClientSessionData {
    fn from(session: OAuthSession) -> Self {
        ClientSessionData {
            account_did: Did::new_owned(session.account_did).expect("stored DID should be valid"),
            session_id: SmolStr::from(session.session_id),
            host_url: session.host_url,
            authserver_url: SmolStr::from(session.authserver_url),
            authserver_token_endpoint: SmolStr::from(session.authserver_token_endpoint),
            authserver_revocation_endpoint: session
                .authserver_revocation_endpoint
                .map(SmolStr::from),
            scopes: Scopes::new(SmolStr::from(session.scopes.as_str()))
                .expect("stored scopes should be valid"),
            dpop_data: DpopClientData {
                dpop_key: session.dpop_key,
                dpop_authserver_nonce: SmolStr::from(session.dpop_authserver_nonce),
                dpop_host_nonce: SmolStr::from(session.dpop_host_nonce),
            },
            token_set: jacquard_oauth::types::TokenSet {
                iss: SmolStr::from(session.iss),
                sub: Did::new_owned(session.sub).expect("stored DID should be valid"),
                aud: SmolStr::from(session.aud),
                scope: session.scope.map(SmolStr::from),
                refresh_token: session.refresh_token.map(SmolStr::from),
                access_token: SmolStr::from(session.access_token),
                token_type: session.token_type,
                expires_at: session.expires_at,
            },
            #[cfg(feature = "scope-check")]
            resolved_scopes: None,
        }
    }
}

/// Persisted OAuth authorization request state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthState {
    /// Random identifier generated for the authorization flow (`state`)
    pub state: String,

    /// Base URL of the authorization server (PDS or entryway)
    pub authserver_url: Uri<String>,

    /// Optional pre-known account DID
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub account_did: Option<String>,

    /// Requested scopes (space-separated, normalized).
    pub scopes: String,

    /// Request URI for the authorization step
    pub request_uri: String,

    /// Full token endpoint URL
    pub authserver_token_endpoint: String,

    /// Full revocation endpoint URL, if available
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub authserver_revocation_endpoint: Option<String>,

    /// PKCE verifier
    pub pkce_verifier: String,

    /// Client DPoP key material
    pub dpop_key: Key,
    /// Auth server DPoP nonce at PAR time
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub dpop_authserver_nonce: Option<String>,
}

impl<S: jacquard_common::bos::BosStr + Ord> TryFrom<AuthRequestData<S>> for OAuthState {
    type Error = jacquard_common::deps::fluent_uri::ParseError;

    fn try_from(value: AuthRequestData<S>) -> Result<Self, Self::Error> {
        Ok(OAuthState {
            authserver_url: Uri::parse(value.authserver_url.as_ref())?.to_owned(),
            account_did: value
                .account_did
                .map(|s| AsRef::<str>::as_ref(&s).to_owned()),
            scopes: String::from(value.scopes.to_normalized_string()),
            request_uri: AsRef::<str>::as_ref(&value.request_uri).to_owned(),
            authserver_token_endpoint: AsRef::<str>::as_ref(&value.authserver_token_endpoint)
                .to_owned(),
            authserver_revocation_endpoint: value
                .authserver_revocation_endpoint
                .map(|s| AsRef::<str>::as_ref(&s).to_owned()),
            pkce_verifier: AsRef::<str>::as_ref(&value.pkce_verifier).to_owned(),
            dpop_key: value.dpop_data.dpop_key,
            dpop_authserver_nonce: value
                .dpop_data
                .dpop_authserver_nonce
                .map(|s| AsRef::<str>::as_ref(&s).to_owned()),
            state: AsRef::<str>::as_ref(&value.state).to_owned(),
        })
    }
}

impl From<OAuthState> for AuthRequestData {
    fn from(value: OAuthState) -> Self {
        AuthRequestData {
            authserver_url: SmolStr::from(value.authserver_url.as_str()),
            state: SmolStr::from(value.state),
            account_did: value
                .account_did
                .map(|s| Did::new_owned(s).expect("stored DID should be valid")),
            authserver_revocation_endpoint: value.authserver_revocation_endpoint.map(SmolStr::from),
            scopes: Scopes::new(SmolStr::from(value.scopes.as_str()))
                .expect("stored scopes should be valid"),
            request_uri: SmolStr::from(value.request_uri),
            authserver_token_endpoint: SmolStr::from(value.authserver_token_endpoint),
            pkce_verifier: SmolStr::from(value.pkce_verifier),
            dpop_data: DpopReqData {
                dpop_key: value.dpop_key,
                dpop_authserver_nonce: value.dpop_authserver_nonce.map(SmolStr::from),
            },
        }
    }
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

    fn atp_key(key: &SessionKey) -> String {
        format!("atp:{}", key)
    }

    fn oauth_key(key: &SessionKey) -> String {
        format!("oauth:{}", key)
    }

    fn oauth_state_key(state: &str) -> String {
        format!("oauth-state:{}", state)
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
        if let StoredSession::OAuth(session) = serde_json::from_value(value)? {
            Ok(Some(session.into()))
        } else {
            Ok(None)
        }
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        let key = SessionKey::new(session.account_did.clone(), session.session_id.clone());
        self.0.set_value(
            Self::oauth_key(&key),
            serde_json::to_value(StoredSession::OAuth(session.into()))?,
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
        if let StoredSession::OAuthState(auth_req) = serde_json::from_value(value)? {
            Ok(Some(auth_req.into()))
        } else {
            Ok(None)
        }
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        let key = Self::oauth_state_key(&auth_req_info.state);
        let state = auth_req_info.clone().try_into().map_err(
            |e: jacquard_common::deps::fluent_uri::ParseError| {
                SessionStoreError::Other(Box::new(e))
            },
        )?;
        self.0
            .set_value(key, serde_json::to_value(StoredSession::OAuthState(state))?)?;
        Ok(())
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        let key = Self::oauth_state_key(state);
        self.0.remove_value(&key)
    }

    async fn list_session_keys(&self) -> Result<Vec<SessionKey>, SessionStoreError> {
        let mut keys = Vec::new();
        for (_key, value) in self.0.entries()? {
            if let Ok(StoredSession::OAuth(session)) =
                serde_json::from_value::<StoredSession>(value)
            {
                keys.push(SessionKey::new(
                    Did::new_owned(session.account_did).expect("stored DID should be valid"),
                    session.session_id,
                ));
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
            session_id: key.session_id.to_string(),
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

    async fn select_session(
        &self,
        hint: &SessionHint,
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

    async fn select_session(
        &self,
        hint: &SessionHint,
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
    use jacquard_common::types::string::{Did, Handle};
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
            #[cfg(feature = "scope-check")]
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
