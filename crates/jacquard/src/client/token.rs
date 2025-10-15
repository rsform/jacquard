use jacquard_common::IntoStatic;
use jacquard_common::cowstr::ToCowStr;
use jacquard_common::session::{FileTokenStore, SessionStore, SessionStoreError};
use jacquard_common::types::string::{Datetime, Did};
use jacquard_oauth::scopes::Scope;
use jacquard_oauth::session::{AuthRequestData, ClientSessionData, DpopClientData, DpopReqData};
use jacquard_oauth::types::OAuthTokenType;
use jose_jwk::Key;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

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

/// Minimal persisted representation of an app‑password session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredAtSession {
    /// Access token (JWT)
    access_jwt: String,
    /// Refresh token (JWT)
    refresh_jwt: String,
    /// Account DID
    did: String,
    /// Optional PDS endpoint for faster resume
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pds: Option<String>,
    /// Session id label (e.g., "session")
    session_id: String,
    /// Last known handle
    handle: String,
}

/// Persisted OAuth client session (on-disk format).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OAuthSession {
    /// Account DID
    account_did: String,
    /// Client-generated session id (usually auth `state`)
    session_id: String,

    /// Base URL of the resource server (PDS)
    host_url: Url,

    /// Base URL of the authorization server (PDS or entryway)
    authserver_url: Url,

    /// Full token endpoint URL
    authserver_token_endpoint: String,

    /// Full revocation endpoint URL, if available
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    authserver_revocation_endpoint: Option<String>,

    /// Granted scopes
    scopes: Vec<String>,

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

impl From<ClientSessionData<'_>> for OAuthSession {
    fn from(data: ClientSessionData<'_>) -> Self {
        OAuthSession {
            account_did: data.account_did.to_string(),
            session_id: data.session_id.to_string(),
            host_url: data.host_url,
            authserver_url: data.authserver_url,
            authserver_token_endpoint: data.authserver_token_endpoint.to_string(),
            authserver_revocation_endpoint: data
                .authserver_revocation_endpoint
                .map(|s| s.to_string()),
            scopes: data.scopes.into_iter().map(|s| s.to_string()).collect(),
            dpop_key: data.dpop_data.dpop_key,
            dpop_authserver_nonce: data.dpop_data.dpop_authserver_nonce.to_string(),
            dpop_host_nonce: data.dpop_data.dpop_host_nonce.to_string(),
            iss: data.token_set.iss.to_string(),
            sub: data.token_set.sub.to_string(),
            aud: data.token_set.aud.to_string(),
            scope: data.token_set.scope.map(|s| s.to_string()),
            refresh_token: data.token_set.refresh_token.map(|s| s.to_string()),
            access_token: data.token_set.access_token.to_string(),
            token_type: data.token_set.token_type,
            expires_at: data.token_set.expires_at,
        }
    }
}

impl From<OAuthSession> for ClientSessionData<'_> {
    fn from(session: OAuthSession) -> Self {
        ClientSessionData {
            account_did: session.account_did.into(),
            session_id: session.session_id.to_cowstr(),
            host_url: session.host_url,
            authserver_url: session.authserver_url,
            authserver_token_endpoint: session.authserver_token_endpoint.to_cowstr(),
            authserver_revocation_endpoint: session
                .authserver_revocation_endpoint
                .map(|s| s.to_cowstr().into_static()),
            scopes: session
                .scopes
                .into_iter()
                .map(|s| Scope::parse(&s).unwrap().into_static())
                .collect(),
            dpop_data: DpopClientData {
                dpop_key: session.dpop_key,
                dpop_authserver_nonce: session.dpop_authserver_nonce.to_cowstr(),
                dpop_host_nonce: session.dpop_host_nonce.to_cowstr(),
            },
            token_set: jacquard_oauth::types::TokenSet {
                iss: session.iss.into(),
                sub: session.sub.into(),
                aud: session.aud.into(),
                scope: session.scope.map(|s| s.into()),
                refresh_token: session.refresh_token.map(|s| s.into()),
                access_token: session.access_token.into(),
                token_type: session.token_type,
                expires_at: session.expires_at,
            },
        }
        .into_static()
    }
}

/// Persisted OAuth authorization request state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthState {
    /// Random identifier generated for the authorization flow (`state`)
    pub state: String,

    /// Base URL of the authorization server (PDS or entryway)
    pub authserver_url: Url,

    /// Optional pre-known account DID
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub account_did: Option<String>,

    /// Requested scopes
    pub scopes: Vec<String>,

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

impl From<AuthRequestData<'_>> for OAuthState {
    fn from(value: AuthRequestData) -> Self {
        OAuthState {
            authserver_url: value.authserver_url,
            account_did: value.account_did.map(|s| s.to_string()),
            scopes: value.scopes.into_iter().map(|s| s.to_string()).collect(),
            request_uri: value.request_uri.to_string(),
            authserver_token_endpoint: value.authserver_token_endpoint.to_string(),
            authserver_revocation_endpoint: value
                .authserver_revocation_endpoint
                .map(|s| s.to_string()),
            pkce_verifier: value.pkce_verifier.to_string(),
            dpop_key: value.dpop_data.dpop_key,
            dpop_authserver_nonce: value.dpop_data.dpop_authserver_nonce.map(|s| s.to_string()),
            state: value.state.to_string(),
        }
    }
}

impl From<OAuthState> for AuthRequestData<'_> {
    fn from(value: OAuthState) -> Self {
        AuthRequestData {
            authserver_url: value.authserver_url,
            state: value.state.to_cowstr(),
            account_did: value.account_did.map(|s| Did::from(s).into_static()),
            authserver_revocation_endpoint: value
                .authserver_revocation_endpoint
                .map(|s| s.to_cowstr().into_static()),
            scopes: value
                .scopes
                .into_iter()
                .map(|s| Scope::parse(&s).unwrap().into_static())
                .collect(),
            request_uri: value.request_uri.to_cowstr(),
            authserver_token_endpoint: value.authserver_token_endpoint.to_cowstr(),
            pkce_verifier: value.pkce_verifier.to_cowstr(),
            dpop_data: DpopReqData {
                dpop_key: value.dpop_key,
                dpop_authserver_nonce: value
                    .dpop_authserver_nonce
                    .map(|s| s.to_cowstr().into_static()),
            },
        }
        .into_static()
    }
}

/// Convenience wrapper over `FileTokenStore` offering unified storage across auth modes.
pub struct FileAuthStore(FileTokenStore);

impl FileAuthStore {
    /// Create a new file-backed auth store wrapping `FileTokenStore`.
    pub fn new(path: impl AsRef<std::path::Path>) -> Self {
        Self(FileTokenStore::new(path))
    }
}

impl jacquard_oauth::authstore::ClientAuthStore for FileAuthStore {
    async fn get_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData<'_>>, SessionStoreError> {
        let key = format!("{}_{}", did, session_id);
        if let StoredSession::OAuth(session) = self
            .0
            .get(&key)
            .await
            .ok_or(SessionStoreError::Other("not found".into()))?
        {
            Ok(Some(session.into()))
        } else {
            Ok(None)
        }
    }

    async fn upsert_session(
        &self,
        session: ClientSessionData<'_>,
    ) -> Result<(), SessionStoreError> {
        let key = format!("{}_{}", session.account_did, session.session_id);
        self.0
            .set(key, StoredSession::OAuth(session.into()))
            .await?;
        Ok(())
    }

    async fn delete_session(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        let key = format!("{}_{}", did, session_id);
        let file = std::fs::read_to_string(&self.0.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        let key_string = key.to_string();
        if let Some(store) = store.as_object_mut() {
            store.remove(&key_string);

            std::fs::write(&self.0.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData<'_>>, SessionStoreError> {
        let key = format!("authreq_{}", state);
        if let StoredSession::OAuthState(auth_req) = self
            .0
            .get(&key)
            .await
            .ok_or(SessionStoreError::Other("not found".into()))?
        {
            Ok(Some(auth_req.into()))
        } else {
            Ok(None)
        }
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData<'_>,
    ) -> Result<(), SessionStoreError> {
        let key = format!("authreq_{}", auth_req_info.state);
        self.0
            .set(key, StoredSession::OAuthState(auth_req_info.clone().into()))
            .await?;
        Ok(())
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        let key = format!("authreq_{}", state);
        let file = std::fs::read_to_string(&self.0.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        let key_string = key.to_string();
        if let Some(store) = store.as_object_mut() {
            store.remove(&key_string);

            std::fs::write(&self.0.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(SessionStoreError::Other("invalid store".into()))
        }
    }
}

impl FileAuthStore {
    /// Update the persisted PDS endpoint for an app-password session (best-effort).
    pub fn set_atp_pds(
        &self,
        key: &crate::client::credential_session::SessionKey,
        pds: &Url,
    ) -> Result<(), SessionStoreError> {
        let key_str = format!("{}_{}", key.0, key.1);
        let file = std::fs::read_to_string(&self.0.path)?;
        let mut store: Value = serde_json::from_str(&file)?;
        if let Some(map) = store.as_object_mut() {
            if let Some(value) = map.get_mut(&key_str) {
                if let Some(outer) = value.as_object_mut() {
                    if let Some(inner) = outer.get_mut("Atp").and_then(|v| v.as_object_mut()) {
                        inner.insert(
                            "pds".to_string(),
                            serde_json::Value::String(pds.to_string()),
                        );
                        std::fs::write(&self.0.path, serde_json::to_string_pretty(&store)?)?;
                        return Ok(());
                    }
                }
            }
        }
        Err(SessionStoreError::Other("invalid store".into()))
    }

    /// Read the persisted PDS endpoint for an app-password session, if present.
    pub fn get_atp_pds(
        &self,
        key: &crate::client::credential_session::SessionKey,
    ) -> Result<Option<Url>, SessionStoreError> {
        let key_str = format!("{}_{}", key.0, key.1);
        let file = std::fs::read_to_string(&self.0.path)?;
        let store: Value = serde_json::from_str(&file)?;
        if let Some(value) = store.get(&key_str) {
            if let Some(obj) = value.as_object() {
                if let Some(serde_json::Value::Object(inner)) = obj.get("Atp") {
                    if let Some(serde_json::Value::String(pds)) = inner.get("pds") {
                        return Ok(Url::parse(pds).ok());
                    }
                }
            }
        }
        Ok(None)
    }
}

impl
    jacquard_common::session::SessionStore<
        crate::client::credential_session::SessionKey,
        crate::client::AtpSession,
    > for FileAuthStore
{
    async fn get(
        &self,
        key: &crate::client::credential_session::SessionKey,
    ) -> Option<crate::client::AtpSession> {
        let key_str = format!("{}_{}", key.0, key.1);
        if let Some(StoredSession::Atp(stored)) = self.0.get(&key_str).await {
            Some(crate::client::AtpSession {
                access_jwt: stored.access_jwt.into(),
                refresh_jwt: stored.refresh_jwt.into(),
                did: stored.did.into(),
                handle: stored.handle.into(),
            })
        } else {
            None
        }
    }

    async fn set(
        &self,
        key: crate::client::credential_session::SessionKey,
        session: crate::client::AtpSession,
    ) -> Result<(), jacquard_common::session::SessionStoreError> {
        let key_str = format!("{}_{}", key.0, key.1);
        let stored = StoredAtSession {
            access_jwt: session.access_jwt.to_string(),
            refresh_jwt: session.refresh_jwt.to_string(),
            did: session.did.to_string(),
            // pds endpoint is resolved on restore; do not persist
            pds: None,
            session_id: key.1.to_string(),
            handle: session.handle.to_string(),
        };
        self.0.set(key_str, StoredSession::Atp(stored)).await
    }

    async fn del(
        &self,
        key: &crate::client::credential_session::SessionKey,
    ) -> Result<(), jacquard_common::session::SessionStoreError> {
        let key_str = format!("{}_{}", key.0, key.1);
        // Manual removal to mirror existing pattern
        let file = std::fs::read_to_string(&self.0.path)?;
        let mut store: serde_json::Value = serde_json::from_str(&file)?;
        if let Some(map) = store.as_object_mut() {
            map.remove(&key_str);
            std::fs::write(&self.0.path, serde_json::to_string_pretty(&store)?)?;
            Ok(())
        } else {
            Err(jacquard_common::session::SessionStoreError::Other(
                "invalid store".into(),
            ))
        }
    }
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
        p.push(format!("jacquard-test-{}.json", std::process::id()));
        p
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
        };
        let key: SessionKey = (session.did.clone(), "session".into());
        jacquard_common::session::SessionStore::set(&store, key.clone(), session.clone())
            .await
            .unwrap();
        let restored = jacquard_common::session::SessionStore::get(&store, &key)
            .await
            .unwrap();
        assert_eq!(restored.access_jwt.as_ref(), "a");
        // clean up
        let _ = fs::remove_file(&path);
    }
}
