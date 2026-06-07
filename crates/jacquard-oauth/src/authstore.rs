use std::future::Future;
use std::sync::Arc;

use dashmap::DashMap;
use jacquard_common::{
    bos::BosStr,
    session::{SessionHint, SessionKey, SessionSelector, SessionStore, SessionStoreError},
    types::did::Did,
};
use jacquard_identity::resolver::IdentityResolver;
use smol_str::{SmolStr, format_smolstr};

use crate::session::{AuthRequestData, ClientSessionData};

/// OAuth session lookup result with the matched key and session data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthSessionMatch {
    /// Matched session key.
    pub key: SessionKey,
    /// Stored OAuth client session data for the matched key.
    pub session: ClientSessionData,
}

/// Resolver-backed OAuth session selector.
///
/// This adapter keeps selection pluggable: callers can depend on [`SessionSelector`] while stores
/// with better indexing can provide their own selector implementation.
pub struct OAuthSessionSelector<'a, S, R> {
    store: &'a S,
    resolver: &'a R,
}

impl<'a, S, R> OAuthSessionSelector<'a, S, R> {
    /// Create a selector over an OAuth auth store and identity resolver.
    pub fn new(store: &'a S, resolver: &'a R) -> Self {
        Self { store, resolver }
    }
}

impl<S, R> SessionSelector<OAuthSessionMatch> for OAuthSessionSelector<'_, S, R>
where
    S: ClientAuthStore + SessionSelector<OAuthSessionMatch, Error = SessionStoreError> + Sync,
    R: IdentityResolver + Sync,
{
    type Error = SessionStoreError;

    async fn select_session(
        &self,
        hint: &SessionHint,
    ) -> Result<Option<OAuthSessionMatch>, Self::Error> {
        if let Some(matched) = self.store.select_session(hint).await? {
            return Ok(Some(matched));
        }

        let SessionHint::Handle(handle) = hint else {
            return Ok(None);
        };

        let did = self
            .resolver
            .resolve_handle(handle)
            .await
            .map_err(|e| SessionStoreError::Other(Box::new(e)))?;
        self.store.select_session(&SessionHint::Did(did)).await
    }
}

/// Resolve a [`SessionHint`] against an OAuth [`ClientAuthStore`].
///
/// Exact key lookup avoids enumeration. `Any`, `Did`, and `Handle` use
/// [`ClientAuthStore::list_session_keys`] as the generic fallback; stores that need more efficient
/// indexed lookup can add specialized APIs later without changing the common key type.
pub async fn resolve_oauth_session_hint<S, R>(
    store: &S,
    resolver: &R,
    hint: &SessionHint,
) -> Result<Option<OAuthSessionMatch>, SessionStoreError>
where
    S: ClientAuthStore + SessionSelector<OAuthSessionMatch, Error = SessionStoreError> + Sync,
    R: IdentityResolver + Sync,
{
    OAuthSessionSelector::new(store, resolver)
        .select_session(hint)
        .await
}

async fn oauth_match_for_did<S, D>(
    store: &S,
    did: &Did<D>,
) -> Result<Option<OAuthSessionMatch>, SessionStoreError>
where
    S: ClientAuthStore,
    D: BosStr + Send + Sync,
{
    for key in store.list_session_keys().await? {
        if key.did.as_str() == did.as_ref() {
            if let Some(matched) = oauth_match_for_key(store, key).await? {
                return Ok(Some(matched));
            }
        }
    }
    Ok(None)
}

async fn oauth_match_for_key<S>(
    store: &S,
    key: SessionKey,
) -> Result<Option<OAuthSessionMatch>, SessionStoreError>
where
    S: ClientAuthStore,
{
    Ok(store
        .get_session(&key.did, key.session_id.as_str())
        .await?
        .map(|session| OAuthSessionMatch { key, session }))
}

/// Persistent storage backend for OAuth client sessions and in-flight authorization requests.
///
/// Implementors are responsible for durably storing two categories of data:
/// - Active client sessions (access tokens, refresh tokens, nonces) keyed by DID + session ID.
/// - Pending authorization request state, keyed by the OAuth `state` parameter, which must
///   survive the round-trip to the authorization server and be cleaned up after use.
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait ClientAuthStore {
    /// Retrieve an active session for the given DID and session identifier, if one exists.
    fn get_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<ClientSessionData>, SessionStoreError>>;

    /// Insert or update a session, replacing any existing entry for the same DID and session ID.
    fn upsert_session(
        &self,
        session: ClientSessionData,
    ) -> impl Future<Output = Result<(), SessionStoreError>>;

    /// Delete the session for the given DID and session identifier.
    fn delete_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> impl Future<Output = Result<(), SessionStoreError>>;

    /// Retrieve the authorization request data associated with the given OAuth `state` value.
    fn get_auth_req_info(
        &self,
        state: &str,
    ) -> impl Future<Output = Result<Option<AuthRequestData>, SessionStoreError>>;

    /// Persist authorization request data so it can be retrieved after the OAuth redirect.
    fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> impl Future<Output = Result<(), SessionStoreError>>;

    /// Remove authorization request data after the callback has been handled.
    fn delete_auth_req_info(
        &self,
        state: &str,
    ) -> impl Future<Output = Result<(), SessionStoreError>>;

    /// List active OAuth session keys when the backend supports enumeration.
    fn list_session_keys(
        &self,
    ) -> impl Future<Output = Result<Vec<SessionKey>, SessionStoreError>> {
        async { Ok(Vec::new()) }
    }
}

/// An in-memory implementation of [`ClientAuthStore`], suitable for testing and single-process
/// deployments where session persistence across restarts is not required.
pub struct MemoryAuthStore {
    sessions: DashMap<SmolStr, ClientSessionData>,
    auth_reqs: DashMap<SmolStr, AuthRequestData>,
}

impl MemoryAuthStore {
    /// Create a new, empty in-memory auth store.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            auth_reqs: DashMap::new(),
        }
    }
}

impl ClientAuthStore for MemoryAuthStore {
    async fn get_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<Option<ClientSessionData>, SessionStoreError> {
        let key = format_smolstr!("{}/{}", did, session_id);
        Ok(self.sessions.get(&key).map(|v| v.clone()))
    }

    async fn upsert_session(&self, session: ClientSessionData) -> Result<(), SessionStoreError> {
        let key = format_smolstr!("{}/{}", session.account_did, session.session_id);
        self.sessions.insert(key, session);
        Ok(())
    }

    async fn delete_session<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), SessionStoreError> {
        let key = format_smolstr!("{}/{}", did, session_id);
        self.sessions.remove(&key);
        Ok(())
    }

    async fn get_auth_req_info(
        &self,
        state: &str,
    ) -> Result<Option<AuthRequestData>, SessionStoreError> {
        Ok(self.auth_reqs.get(state).map(|v| v.clone()))
    }

    async fn save_auth_req_info(
        &self,
        auth_req_info: &AuthRequestData,
    ) -> Result<(), SessionStoreError> {
        self.auth_reqs
            .insert(auth_req_info.state.clone(), auth_req_info.clone());
        Ok(())
    }

    async fn delete_auth_req_info(&self, state: &str) -> Result<(), SessionStoreError> {
        self.auth_reqs.remove(state);
        Ok(())
    }

    async fn list_session_keys(&self) -> Result<Vec<SessionKey>, SessionStoreError> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|entry| {
                let session = entry.value();
                SessionKey::new(session.account_did.clone(), session.session_id.clone())
            })
            .collect::<Vec<_>>();
        sessions.sort();
        Ok(sessions)
    }
}

impl SessionSelector<OAuthSessionMatch> for MemoryAuthStore {
    type Error = SessionStoreError;

    async fn select_session(
        &self,
        hint: &SessionHint,
    ) -> Result<Option<OAuthSessionMatch>, Self::Error> {
        match hint {
            SessionHint::Any => {
                let Some(key) = self.list_session_keys().await?.into_iter().next() else {
                    return Ok(None);
                };
                oauth_match_for_key(self, key).await
            }
            SessionHint::Did(did) => oauth_match_for_did(self, did).await,
            SessionHint::Handle(_) | SessionHint::Identifier(_) => Ok(None),
            SessionHint::Key(key) => oauth_match_for_key(self, key.clone()).await,
        }
    }
}

impl<T: ClientAuthStore + Send + Sync> SessionStore<SessionKey, ClientSessionData> for Arc<T> {
    /// Get the current session if present.
    async fn get(&self, key: &SessionKey) -> Option<ClientSessionData> {
        self.as_ref()
            .get_session(&key.did, key.session_id.as_str())
            .await
            .ok()
            .flatten()
    }
    /// Persist the given session.
    async fn set(
        &self,
        _key: SessionKey,
        session: ClientSessionData,
    ) -> Result<(), SessionStoreError> {
        self.as_ref().upsert_session(session).await
    }
    /// Delete the given session.
    async fn del(&self, key: &SessionKey) -> Result<(), SessionStoreError> {
        self.as_ref()
            .delete_session(&key.did, key.session_id.as_str())
            .await
    }

    async fn list_keys(&self) -> Result<Vec<SessionKey>, SessionStoreError> {
        self.as_ref().list_session_keys().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jacquard_common::deps::fluent_uri::Uri;

    use crate::scopes::Scopes;
    use crate::session::DpopClientData;
    use crate::types::{OAuthTokenType, TokenSet};

    fn client_session(did: &'static str, session_id: &'static str) -> ClientSessionData {
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
                dpop_key: crate::utils::generate_key(&[SmolStr::new_static("ES256")]).unwrap(),
                dpop_authserver_nonce: SmolStr::default(),
                dpop_host_nonce: SmolStr::default(),
            },
            token_set: TokenSet {
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
    async fn memory_auth_store_lists_session_keys() {
        let store = MemoryAuthStore::new();
        let session = client_session("did:plc:alice", "state");
        store.upsert_session(session).await.unwrap();

        assert_eq!(
            store.list_session_keys().await.unwrap(),
            vec![SessionKey::new(
                Did::new_static("did:plc:alice").unwrap(),
                "state"
            )]
        );
    }

    #[tokio::test]
    async fn memory_auth_store_selects_sessions_without_identifier_fallback() {
        let store = MemoryAuthStore::new();
        let alice = client_session("did:plc:alice", "state-a");
        let alice_key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "state-a");
        store.upsert_session(alice.clone()).await.unwrap();
        store
            .upsert_session(client_session("did:plc:bob", "state-b"))
            .await
            .unwrap();

        let matched = store
            .select_session(&SessionHint::Any)
            .await
            .unwrap()
            .expect("any match");
        assert_eq!(matched.key, alice_key);
        assert_eq!(matched.session, alice);

        let matched = store
            .select_session(&SessionHint::Did(Did::new_static("did:plc:alice").unwrap()))
            .await
            .unwrap()
            .expect("did match");
        assert_eq!(matched.key, alice_key);

        let matched = store
            .select_session(&SessionHint::Key(alice_key.clone()))
            .await
            .unwrap()
            .expect("key match");
        assert_eq!(matched.key, alice_key);

        assert!(
            store
                .select_session(&SessionHint::Identifier("alice@example.com".into()))
                .await
                .unwrap()
                .is_none(),
            "identifier hints must not fall back to Any"
        );
    }

    #[derive(Clone, Default)]
    struct CountingResolver {
        handle_calls: Arc<tokio::sync::RwLock<usize>>,
    }

    impl IdentityResolver for CountingResolver {
        fn options(&self) -> &jacquard_identity::resolver::ResolverOptions {
            use std::sync::LazyLock;
            static OPTS: LazyLock<jacquard_identity::resolver::ResolverOptions> =
                LazyLock::new(jacquard_identity::resolver::ResolverOptions::default);
            &OPTS
        }

        async fn resolve_handle<S: BosStr + Sync>(
            &self,
            _handle: &jacquard_common::types::string::Handle<S>,
        ) -> Result<Did, jacquard_identity::resolver::IdentityError> {
            *self.handle_calls.write().await += 1;
            Ok(Did::new_static("did:plc:alice").unwrap())
        }

        async fn resolve_did_doc<S: BosStr + Sync>(
            &self,
            _did: &Did<S>,
        ) -> Result<
            jacquard_identity::resolver::DidDocResponse,
            jacquard_identity::resolver::IdentityError,
        > {
            unreachable!("OAuth selector tests do not resolve DID documents")
        }
    }

    #[tokio::test]
    async fn oauth_session_selector_uses_store_before_handle_resolution() {
        let store = MemoryAuthStore::new();
        let resolver = CountingResolver::default();
        let alice = client_session("did:plc:alice", "state");
        store.upsert_session(alice.clone()).await.unwrap();

        assert!(
            OAuthSessionSelector::new(&store, &resolver)
                .select_session(&SessionHint::Identifier("alice@example.com".into()))
                .await
                .unwrap()
                .is_none(),
            "identifier hints should not trigger resolver fallback"
        );
        assert_eq!(*resolver.handle_calls.read().await, 0);

        let matched = OAuthSessionSelector::new(&store, &resolver)
            .select_session(&SessionHint::Handle(
                jacquard_common::types::string::Handle::new_static("alice.bsky.social").unwrap(),
            ))
            .await
            .unwrap()
            .expect("resolver fallback DID match");
        assert_eq!(matched.session, alice);
        assert_eq!(*resolver.handle_calls.read().await, 1);
    }

    #[tokio::test]
    async fn arc_memory_auth_store_is_session_store() {
        let store = Arc::new(MemoryAuthStore::new());
        let session = client_session("did:plc:alice", "state");
        let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "state");

        SessionStore::set(&store, key.clone(), session.clone())
            .await
            .unwrap();
        assert_eq!(SessionStore::get(&store, &key).await, Some(session));
        assert_eq!(
            SessionStore::list_keys(&store).await.unwrap(),
            vec![key.clone()]
        );
        SessionStore::del(&store, &key).await.unwrap();
        assert_eq!(SessionStore::get(&store, &key).await, None);
    }
}
