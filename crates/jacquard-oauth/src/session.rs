use std::{str::FromStr, sync::Arc};

use chrono::TimeDelta;

use crate::{
    atproto::{AtprotoClientMetadata, atproto_client_metadata},
    authstore::ClientAuthStore,
    dpop::DpopExt,
    keyset::Keyset,
    request::{OAuthMetadata, refresh},
    resolver::OAuthResolver,
    scopes::Scopes,
    types::TokenSet,
};

use dashmap::DashMap;
use jacquard_common::{
    IntoStatic,
    bos::{BosStr, DefaultStr},
    deps::fluent_uri::Uri,
    http_client::HttpClient,
    session::SessionStoreError,
    types::{did::Did, string::Datetime},
};
use jose_jwk::Key;
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr, format_smolstr};
use tokio::sync::Mutex;

/// Provides DPoP key material and per-server nonces to the DPoP proof-building machinery.
///
/// This trait abstracts over two different holders of DPoP state: [`DpopReqData`] (used
/// during the initial authorization request, where only an authserver nonce is tracked) and
/// [`DpopClientData`] (used in active sessions, where both authserver and host nonces are
/// maintained). Implementors must store nonces durably so that the next request to the same
/// server includes the most recently observed nonce.
pub trait DpopDataSource {
    /// Return the private JWK used to sign DPoP proofs.
    fn key(&self) -> &Key;
    /// Return the most recently observed nonce from the authorization server, if any.
    fn authserver_nonce(&self) -> Option<&str>;
    /// Persist a new nonce received from the authorization server.
    fn set_authserver_nonce(&mut self, nonce: SmolStr);
    /// Return the most recently observed nonce from the resource server (PDS), if any.
    fn host_nonce(&self) -> Option<&str>;
    /// Persist a new nonce received from the resource server (PDS).
    fn set_host_nonce(&mut self, nonce: SmolStr);
}

/// Persisted information about an OAuth session. Used to resume an active session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S: serde::Serialize + BosStr + Ord",
    deserialize = "S: serde::Deserialize<'de> + BosStr + AsRef<str>"
))]
pub struct ClientSessionData<S: BosStr = DefaultStr> {
    /// DID of the authenticated account; serves as the primary key for session storage
    /// because only one active session per account is assumed.
    pub account_did: Did<S>,

    /// Opaque identifier that distinguishes this session from other sessions for the same account.
    ///
    /// Reuses the random `state` token generated during the PAR flow.
    pub session_id: S,

    /// Base URL of the resource server (PDS): scheme, host, and port only
    pub host_url: Uri<String>,

    /// Base URL of the authorization server (PDS or entryway): scheme, host, and port only
    pub authserver_url: S,

    /// Full URL of the authorization server's token endpoint.
    pub authserver_token_endpoint: S,

    /// Full URL of the authorization server's revocation endpoint, if advertised.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub authserver_revocation_endpoint: Option<S>,

    /// The set of OAuth scopes approved for this session, as returned in the initial token response.
    pub scopes: Scopes<S>,

    /// DPoP key and nonce state for ongoing requests in this session.
    #[serde(flatten)]
    pub dpop_data: DpopClientData,

    /// Current token set (access token, refresh token, expiry, etc.).
    #[serde(flatten)]
    pub token_set: TokenSet<S>,

    /// Fully expanded scopes with include scopes resolved.
    ///
    /// This is populated eagerly at session creation when `scope-check` is enabled.
    /// It is `None` when scope checking is not enabled, when no eager resolution was
    /// performed, or when reading older persisted sessions that predate this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_scopes: Option<Vec<crate::scopes::Scope<smol_str::SmolStr>>>,
}

impl<S: BosStr + Ord + IntoStatic + AsRef<str>> IntoStatic for ClientSessionData<S>
where
    S::Output: BosStr + Ord + AsRef<str>,
{
    type Output = ClientSessionData<S::Output>;

    fn into_static(self) -> Self::Output {
        let resolved_scopes = self.resolved_scopes;

        ClientSessionData {
            authserver_url: self.authserver_url.into_static(),
            authserver_token_endpoint: self.authserver_token_endpoint.into_static(),
            authserver_revocation_endpoint: self
                .authserver_revocation_endpoint
                .map(IntoStatic::into_static),
            scopes: self.scopes.into_static(),
            dpop_data: self.dpop_data,
            token_set: self.token_set.into_static(),
            account_did: self.account_did.into_static(),
            session_id: self.session_id.into_static(),
            host_url: self.host_url.clone(),
            resolved_scopes,
        }
    }
}

impl<S: BosStr + Ord + AsRef<str>> ClientSessionData<S> {
    /// Update this session's token set and, if the new token set includes scopes, replace the scope list.
    ///
    /// Called after a successful token refresh so that any scope changes returned by the server
    /// are reflected in the persisted session without requiring a full re-authentication.
    ///
    /// This method is only available on `DefaultStr`-backed sessions (the common case for
    /// in-memory sessions). Zero-copy borrowed sessions are read-only by nature and would
    /// not be refreshed in place.
    pub fn update_with_tokens(&mut self, token_set: &TokenSet<S>)
    where
        S: FromStr + Clone + From<SmolStr> + AsRef<str>,
        S::Err: std::fmt::Debug,
    {
        if let Some(scope_str) = token_set.scope.as_ref() {
            // Parse scopes from the returned scope string, converting to the appropriate backing type
            let scopes_smol = Scopes::new(scope_str.as_ref().to_smolstr())
                .expect("server returned invalid scopes in token refresh");
            self.scopes = scopes_smol.convert();
        }
        self.token_set = token_set.clone();
    }
}

/// DPoP state for an active OAuth session, persisted alongside the token set.
///
/// Both nonces must be written back to the store after each request so that the next
/// request to the same server includes the correct replay-protection nonce.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DpopClientData {
    /// The private JWK bound to this session; used to sign all DPoP proofs.
    pub dpop_key: Key,
    /// Most recently observed DPoP nonce from the authorization server.
    pub dpop_authserver_nonce: SmolStr,
    /// Most recently observed DPoP nonce from the resource server (PDS).
    pub dpop_host_nonce: SmolStr,
}

impl DpopDataSource for DpopClientData {
    fn key(&self) -> &Key {
        &self.dpop_key
    }

    fn authserver_nonce(&self) -> Option<&str> {
        Some(self.dpop_authserver_nonce.as_ref())
    }

    fn host_nonce(&self) -> Option<&str> {
        Some(self.dpop_host_nonce.as_ref())
    }

    fn set_authserver_nonce(&mut self, nonce: SmolStr) {
        self.dpop_authserver_nonce = nonce;
    }

    fn set_host_nonce(&mut self, nonce: SmolStr) {
        self.dpop_host_nonce = nonce;
    }
}

/// Transient state created during the PAR flow and consumed by the callback handler.
///
/// This struct is persisted to the auth store between [`crate::request::par`] and
/// [`crate::client::OAuthClient::callback`] so that the callback can verify the
/// `state`, reconstruct the token exchange, and create a full [`ClientSessionData`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S: serde::Serialize + BosStr + Ord",
    deserialize = "S: serde::Deserialize<'de> + BosStr + AsRef<str>"
))]
pub struct AuthRequestData<S: BosStr = DefaultStr> {
    /// Random identifier generated for this authorization request; used as the primary key
    /// for storing and looking up this record during the callback.
    pub state: S,

    /// Base URL of the authorization server that was selected for this flow.
    pub authserver_url: S,

    /// If the flow was initiated with a DID or handle, the resolved DID is stored here
    /// so it can be compared against the `sub` in the token response.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub account_did: Option<Did<S>>,

    /// OAuth scopes requested for this authorization.
    pub scopes: Scopes<S>,

    /// The PAR `request_uri` returned by the authorization server; included in the redirect URL.
    pub request_uri: S,

    /// Full URL of the authorization server's token endpoint.
    pub authserver_token_endpoint: S,

    /// Full URL of the authorization server's revocation endpoint, if advertised.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub authserver_revocation_endpoint: Option<S>,

    /// The PKCE code verifier whose SHA-256 hash was sent as the code challenge; required
    /// at the token exchange step to prove the initiator of the auth request.
    pub pkce_verifier: S,

    /// DPoP key and any authserver nonce observed during the PAR request.
    #[serde(flatten)]
    pub dpop_data: DpopReqData,
}

impl<S: BosStr + Ord + IntoStatic + AsRef<str>> IntoStatic for AuthRequestData<S>
where
    S::Output: BosStr + Ord + AsRef<str>,
{
    type Output = AuthRequestData<S::Output>;

    fn into_static(self) -> AuthRequestData<S::Output> {
        AuthRequestData {
            request_uri: self.request_uri.into_static(),
            authserver_token_endpoint: self.authserver_token_endpoint.into_static(),
            authserver_revocation_endpoint: self
                .authserver_revocation_endpoint
                .map(|s| s.into_static()),
            pkce_verifier: self.pkce_verifier.into_static(),
            dpop_data: self.dpop_data,
            state: self.state.into_static(),
            authserver_url: self.authserver_url.into_static(),
            account_did: self.account_did.into_static(),
            scopes: self.scopes.into_static(),
        }
    }
}

/// DPoP state for an in-progress authorization request (PAR through code exchange).
///
/// Unlike [`DpopClientData`], this struct only tracks the authserver nonce—no resource-server
/// nonce is needed until a full session is established.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DpopReqData {
    /// The private JWK generated fresh for this authorization request and session.
    pub dpop_key: Key,
    /// DPoP nonce received from the authorization server during the PAR exchange, if any.
    pub dpop_authserver_nonce: Option<SmolStr>,
}

impl DpopDataSource for DpopReqData {
    fn key(&self) -> &Key {
        &self.dpop_key
    }

    fn authserver_nonce(&self) -> Option<&str> {
        self.dpop_authserver_nonce.as_ref().map(|n| n.as_ref())
    }

    fn host_nonce(&self) -> Option<&str> {
        None
    }

    fn set_authserver_nonce(&mut self, nonce: SmolStr) {
        self.dpop_authserver_nonce = Some(nonce);
    }

    fn set_host_nonce(&mut self, _nonce: SmolStr) {}
}

/// Static configuration for an OAuth client: the signing keyset and registered client metadata.
///
/// `ClientData` is constructed once at startup and shared (via `Arc`) across all sessions
/// managed by the same [`crate::client::OAuthClient`].
#[derive(Clone, Debug)]
pub struct ClientData<S>
where
    S: BosStr + FromStr + Ord,
    <S as FromStr>::Err: core::fmt::Debug,
{
    /// Optional private key set used for `private_key_jwt` client authentication.
    /// When `None`, the `none` authentication method is used instead.
    pub keyset: Option<Keyset>,
    /// AT Protocol-specific client registration metadata (redirect URIs, scopes, etc.).
    pub config: AtprotoClientMetadata<S>,
}

impl<S> IntoStatic for ClientData<S>
where
    S: BosStr + FromStr + Ord + IntoStatic,
    S::Output: BosStr + Ord + FromStr,
    <S as FromStr>::Err: core::fmt::Debug,
    <S::Output as FromStr>::Err: core::fmt::Debug,
{
    type Output = ClientData<S::Output>;
    fn into_static(self) -> ClientData<S::Output> {
        ClientData {
            keyset: self.keyset,
            config: self.config.into_static(),
        }
    }
}

impl<S: BosStr + FromStr + Ord> ClientData<S>
where
    <S as FromStr>::Err: core::fmt::Debug,
{
    /// Create `ClientData` with an optional signing keyset and the given client metadata.
    pub fn new(keyset: Option<Keyset>, config: AtprotoClientMetadata<S>) -> Self {
        Self { keyset, config }
    }

    /// Create `ClientData` without a signing keyset, relying on the `none` auth method.
    ///
    /// Suitable for public clients (e.g., single-page applications or native apps) that
    /// cannot securely store a private key.
    pub fn new_public(config: AtprotoClientMetadata<S>) -> Self {
        Self {
            keyset: None,
            config,
        }
    }
}

/// A bundle of client configuration and an active session, used for operations that need both.
///
/// `ClientSession` is a convenience type that pairs a [`ClientData`] with a
/// [`ClientSessionData`] so that methods like `metadata` can access both without requiring
/// callers to pass them separately.
pub struct ClientSession<S: BosStr = DefaultStr>
where
    S: FromStr + Ord,
    <S as FromStr>::Err: core::fmt::Debug,
{
    /// Optional signing keyset, forwarded from [`ClientData`].
    pub keyset: Option<Keyset>,
    /// Client registration metadata, forwarded from [`ClientData`].
    pub config: AtprotoClientMetadata<S>,
    /// The session state for the authenticated account.
    pub session_data: ClientSessionData<S>,
}

impl<S: BosStr> ClientSession<S>
where
    S: FromStr + Ord + Clone,
    <S as FromStr>::Err: core::fmt::Debug,
{
    /// Construct a `ClientSession` from a [`ClientData`] and an active session.
    pub fn new(
        ClientData { keyset, config }: ClientData<S>,
        session_data: ClientSessionData<S>,
    ) -> Self {
        Self {
            keyset,
            config,
            session_data,
        }
    }

    /// Fetch and assemble an [`OAuthMetadata`] for the authorization server of this session.
    pub async fn metadata<T: HttpClient + OAuthResolver + Send + Sync>(
        &self,
        client: &T,
    ) -> Result<OAuthMetadata<S>, Error>
    where
        S: IntoStatic,
    {
        Ok(OAuthMetadata {
            server_metadata: client
                .get_authorization_server_metadata(self.session_data.authserver_url.as_ref())
                .await
                .map_err(|e| Error::ServerAgent(crate::request::RequestError::resolver(e)))?,
            client_metadata: atproto_client_metadata(&self.config, &self.keyset).unwrap(),
            keyset: self.keyset.clone(),
        })
    }
}

/// Errors that can occur during OAuth session management.
#[derive(thiserror::Error, Debug, miette::Diagnostic)]
#[non_exhaustive]
pub enum Error {
    /// A token-endpoint or metadata operation failed.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::session::request))]
    ServerAgent(#[from] crate::request::RequestError),
    /// The backing session store returned an error.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::session::storage))]
    Store(#[from] SessionStoreError),
    /// The requested session does not exist in the store.
    #[error("session does not exist")]
    #[diagnostic(code(jacquard_oauth::session::not_found))]
    SessionNotFound,
    /// Token refresh failed with a permanent error (e.g., `invalid_grant`); the session
    /// has already been removed from the store and the user must re-authenticate.
    #[error("session refresh failed permanently")]
    #[diagnostic(
        code(jacquard_oauth::session::refresh_failed),
        help("the session has been cleared - user must re-authenticate")
    )]
    RefreshFailed(#[source] crate::request::RequestError),
}

impl Error {
    /// Returns true if this error indicates a permanent auth failure
    /// where the user needs to re-authenticate.
    pub fn is_permanent(&self) -> bool {
        match self {
            Error::RefreshFailed(_) => true,
            Error::SessionNotFound => true,
            Error::ServerAgent(e) => e.is_permanent(),
            Error::Store(_) => false,
        }
    }
}

/// Central coordinator for OAuth session storage and token refresh.
///
/// `SessionRegistry` wraps the [`ClientAuthStore`] and provides serialized token refresh:
/// concurrent refresh attempts for the same `(DID, session_id)` pair are coalesced behind
/// a per-key `Mutex` stored in `pending`, so only one refresh request is issued to the
/// authorization server even when many concurrent requests detect an expired token.
pub struct SessionRegistry<T, S, Str>
where
    T: OAuthResolver,
    S: ClientAuthStore,
    Str: BosStr + FromStr + Ord,
    <Str as FromStr>::Err: core::fmt::Debug,
{
    /// Backing store for persisting session data across process restarts.
    pub store: Arc<S>,
    /// Shared resolver used to fetch authorization server metadata during refresh.
    pub client: Arc<T>,
    /// Static client configuration (keyset and registration metadata).
    pub client_data: ClientData<Str>,
    /// Per-`(DID, session_id)` mutex that serializes concurrent refresh attempts.
    pending: DashMap<SmolStr, Arc<Mutex<()>>>,
}

impl<T, S, Str> SessionRegistry<T, S, Str>
where
    S: ClientAuthStore,
    T: OAuthResolver,
    Str: BosStr + FromStr + Ord,
    <Str as FromStr>::Err: core::fmt::Debug,
{
    /// Create a new registry, taking ownership of the store.
    pub fn new(store: S, client: Arc<T>, client_data: ClientData<Str>) -> Self {
        let store = Arc::new(store);
        Self {
            store,
            client,
            client_data,
            pending: DashMap::new(),
        }
    }

    /// Create a new registry from an already-`Arc`-wrapped store.
    ///
    /// Use this variant when the store needs to be accessed from outside the registry,
    /// for example to expose session listing or administration functionality.
    pub fn new_shared(store: Arc<S>, client: Arc<T>, client_data: ClientData<Str>) -> Self {
        Self {
            store,
            client,
            client_data,
            pending: DashMap::new(),
        }
    }
}

impl<T, S, Str> SessionRegistry<T, S, Str>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    Str: BosStr + FromStr + Ord + Clone,
    <Str as FromStr>::Err: core::fmt::Debug,
{
    async fn get_refreshed<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<ClientSessionData, Error> {
        let key = format_smolstr!("{}_{}", did, session_id);
        let lock = self
            .pending
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        let session = self
            .store
            .get_session(did, session_id)
            .await?
            .ok_or(Error::SessionNotFound)?;

        // Check if token is still valid with a 60-second buffer before expiry.
        // This triggers proactive refresh before the token actually expires,
        // avoiding the race condition where a token expires mid-request.
        const EXPIRY_BUFFER_SECS: i64 = 60;
        if let Some(expires_at) = &session.token_set.expires_at {
            let now_with_buffer = Datetime::now()
                .as_ref()
                .checked_add_signed(TimeDelta::seconds(EXPIRY_BUFFER_SECS))
                .map(Datetime::new)
                .unwrap_or_else(Datetime::now);
            if expires_at > &now_with_buffer {
                return Ok(session);
            }
        }
        let metadata =
            OAuthMetadata::new(self.client.as_ref(), &self.client_data, &session).await?;
        match refresh(self.client.as_ref(), session, &metadata).await {
            Ok(refreshed) => {
                self.store.upsert_session(refreshed.clone()).await?;
                Ok(refreshed)
            }
            Err(e) if e.is_permanent() => {
                // Session is permanently dead - clean it up
                let _ = self.store.delete_session(did, session_id).await;
                Err(Error::RefreshFailed(e))
            }
            Err(e) => Err(Error::ServerAgent(e)),
        }
    }
    /// Retrieve a session from the store, optionally refreshing it first.
    ///
    /// When `refresh` is `true`, proactively
    /// renews the token if it is within 60 seconds of expiry. When `false`, returns the session
    /// data as-is without contacting the authorization server.
    pub async fn get<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
        refresh: bool,
    ) -> Result<ClientSessionData, Error> {
        if refresh {
            self.get_refreshed(did, session_id).await
        } else {
            // TODO: cached?
            self.store
                .get_session(did, session_id)
                .await?
                .ok_or(Error::SessionNotFound)
        }
    }
    /// Persist an updated session to the backing store.
    pub async fn set(&self, value: ClientSessionData) -> Result<(), Error> {
        self.store.upsert_session(value).await?;
        Ok(())
    }
    /// Delete a session from the backing store.
    pub async fn del<D: BosStr + Send + Sync>(
        &self,
        did: &Did<D>,
        session_id: &str,
    ) -> Result<(), Error> {
        self.store.delete_session(did, session_id).await?;
        Ok(())
    }
}
