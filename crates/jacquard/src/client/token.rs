use jacquard_common::IntoStatic;
use jacquard_common::cowstr::ToCowStr;
use jacquard_common::session::{FileTokenStore, SessionStore, SessionStoreError};
use jacquard_common::types::string::{Datetime, Did, Handle};
use jacquard_oauth::scopes::Scope;
use jacquard_oauth::session::{AuthRequestData, ClientSessionData, DpopClientData, DpopReqData};
use jacquard_oauth::types::OAuthTokenType;
use jose_jwk::Key;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Display;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum StoredSession {
    Atp(StoredAtSession),
    OAuth(OAuthSession),
    OAuthState(OAuthState),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct StoredAtSession {
    access_jwt: String,
    refresh_jwt: String,
    did: String,
    pds: String,
    session_id: String,
    handle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct OAuthSession {
    account_did: String,
    session_id: String,

    // Base URL of the "resource server" (eg, PDS). Should include scheme, hostname, port; no path or auth info.
    host_url: Url,

    // Base URL of the "auth server" (eg, PDS or entryway). Should include scheme, hostname, port; no path or auth info.
    authserver_url: Url,

    // Full token endpoint
    authserver_token_endpoint: String,

    // Full revocation endpoint, if it exists
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    authserver_revocation_endpoint: Option<String>,

    // The set of scopes approved for this session (returned in the initial token request)
    scopes: Vec<String>,

    pub dpop_key: Key,
    // Current auth server DPoP nonce
    pub dpop_authserver_nonce: String,
    // Current host ("resource server", eg PDS) DPoP nonce
    pub dpop_host_nonce: String,

    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub scope: Option<String>,

    pub refresh_token: Option<String>,
    pub access_token: String,
    pub token_type: OAuthTokenType,

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthState {
    // The random identifier generated by the client for the auth request flow. Can be used as "primary key" for storing and retrieving this information.
    pub state: String,

    // URL of the auth server (eg, PDS or entryway)
    pub authserver_url: Url,

    // If the flow started with an account identifier (DID or handle), it should be persisted, to verify against the initial token response.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub account_did: Option<String>,

    // OAuth scope strings
    pub scopes: Vec<String>,

    // unique token in URI format, which will be used by the client in the auth flow redirect
    pub request_uri: String,

    // Full token endpoint URL
    pub authserver_token_endpoint: String,

    // Full revocation endpoint, if it exists
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub authserver_revocation_endpoint: Option<String>,

    // The secret token/nonce which a code challenge was generated from
    pub pkce_verifier: String,

    pub dpop_key: Key,
    // Current auth server DPoP nonce
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

pub struct FileAuthStore(FileTokenStore);

#[async_trait::async_trait]
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
