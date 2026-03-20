use std::sync::Arc;

use chrono::TimeDelta;

use crate::{
    atproto::{AtprotoClientMetadata, atproto_client_metadata},
    authstore::ClientAuthStore,
    dpop::DpopExt,
    keyset::Keyset,
    request::{OAuthMetadata, refresh},
    resolver::OAuthResolver,
    scopes::Scope,
    types::TokenSet,
};

use dashmap::DashMap;
use jacquard_common::{
    CowStr, IntoStatic,
    deps::fluent_uri::Uri,
    http_client::HttpClient,
    session::SessionStoreError,
    types::{did::Did, string::Datetime},
};
use jose_jwk::Key;
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, format_smolstr};
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
    fn authserver_nonce(&self) -> Option<CowStr<'_>>;
    /// Persist a new nonce received from the authorization server.
    fn set_authserver_nonce(&mut self, nonce: CowStr<'_>);
    /// Return the most recently observed nonce from the resource server (PDS), if any.
    fn host_nonce(&self) -> Option<CowStr<'_>>;
    /// Persist a new nonce received from the resource server (PDS).
    fn set_host_nonce(&mut self, nonce: CowStr<'_>);
}

/// Persisted information about an OAuth session. Used to resume an active session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientSessionData<'s> {
    /// DID of the authenticated account; serves as the primary key for session storage
    /// because only one active session per account is assumed.
    #[serde(borrow)]
    pub account_did: Did<'s>,

    /// Opaque identifier that distinguishes this session from other sessions for the same account.
    ///
    /// Reuses the random `state` token generated during the PAR flow.
    pub session_id: CowStr<'s>,

    /// Base URL of the resource server (PDS): scheme, host, and port only
    pub host_url: Uri<String>,

    /// Base URL of the authorization server (PDS or entryway): scheme, host, and port only
    pub authserver_url: CowStr<'s>,

    /// Full URL of the authorization server's token endpoint.
    pub authserver_token_endpoint: CowStr<'s>,

    /// Full URL of the authorization server's revocation endpoint, if advertised.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub authserver_revocation_endpoint: Option<CowStr<'s>>,

    /// The set of OAuth scopes approved for this session, as returned in the initial token response.
    pub scopes: Vec<Scope<'s>>,

    /// DPoP key and nonce state for ongoing requests in this session.
    #[serde(flatten)]
    pub dpop_data: DpopClientData<'s>,

    /// Current token set (access token, refresh token, expiry, etc.).
    #[serde(flatten)]
    pub token_set: TokenSet<'s>,
}

impl IntoStatic for ClientSessionData<'_> {
    type Output = ClientSessionData<'static>;

    fn into_static(self) -> Self::Output {
        ClientSessionData {
            authserver_url: self.authserver_url.into_static(),
            authserver_token_endpoint: self.authserver_token_endpoint.into_static(),
            authserver_revocation_endpoint: self
                .authserver_revocation_endpoint
                .map(IntoStatic::into_static),
            scopes: self.scopes.into_static(),
            dpop_data: self.dpop_data.into_static(),
            token_set: self.token_set.into_static(),
            account_did: self.account_did.into_static(),
            session_id: self.session_id.into_static(),
            host_url: self.host_url.clone(),
        }
    }
}

impl ClientSessionData<'_> {
    /// Update this session's token set and, if the new token set includes scopes, replace the scope list.
    ///
    /// Called after a successful token refresh so that any scope changes returned by the server
    /// are reflected in the persisted session without requiring a full re-authentication.
    pub fn update_with_tokens(&mut self, token_set: TokenSet<'_>) {
        if let Some(Ok(scopes)) = token_set
            .scope
            .as_ref()
            .map(|scope| Scope::parse_multiple_reduced(&scope).map(IntoStatic::into_static))
        {
            self.scopes = scopes;
        }
        self.token_set = token_set.into_static();
    }
}

/// DPoP state for an active OAuth session, persisted alongside the token set.
///
/// Both nonces must be written back to the store after each request so that the next
/// request to the same server includes the correct replay-protection nonce.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DpopClientData<'s> {
    /// The private JWK bound to this session; used to sign all DPoP proofs.
    pub dpop_key: Key,
    /// Most recently observed DPoP nonce from the authorization server.
    #[serde(borrow)]
    pub dpop_authserver_nonce: CowStr<'s>,
    /// Most recently observed DPoP nonce from the resource server (PDS).
    pub dpop_host_nonce: CowStr<'s>,
}

impl IntoStatic for DpopClientData<'_> {
    type Output = DpopClientData<'static>;

    fn into_static(self) -> Self::Output {
        DpopClientData {
            dpop_key: self.dpop_key,
            dpop_authserver_nonce: self.dpop_authserver_nonce.into_static(),
            dpop_host_nonce: self.dpop_host_nonce.into_static(),
        }
    }
}

impl DpopDataSource for DpopClientData<'_> {
    fn key(&self) -> &Key {
        &self.dpop_key
    }
    fn authserver_nonce(&self) -> Option<CowStr<'_>> {
        Some(self.dpop_authserver_nonce.clone())
    }

    fn host_nonce(&self) -> Option<CowStr<'_>> {
        Some(self.dpop_host_nonce.clone())
    }

    fn set_authserver_nonce(&mut self, nonce: CowStr<'_>) {
        self.dpop_authserver_nonce = nonce.into_static();
    }

    fn set_host_nonce(&mut self, nonce: CowStr<'_>) {
        self.dpop_host_nonce = nonce.into_static();
    }
}

/// Transient state created during the PAR flow and consumed by the callback handler.
///
/// This struct is persisted to the auth store between [`crate::request::par`] and
/// [`crate::client::OAuthClient::callback`] so that the callback can verify the
/// `state`, reconstruct the token exchange, and create a full [`ClientSessionData`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRequestData<'s> {
    /// Random identifier generated for this authorization request; used as the primary key
    /// for storing and looking up this record during the callback.
    #[serde(borrow)]
    pub state: CowStr<'s>,

    /// Base URL of the authorization server that was selected for this flow.
    pub authserver_url: CowStr<'s>,

    /// If the flow was initiated with a DID or handle, the resolved DID is stored here
    /// so it can be compared against the `sub` in the token response.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub account_did: Option<Did<'s>>,

    /// OAuth scopes requested for this authorization.
    pub scopes: Vec<Scope<'s>>,

    /// The PAR `request_uri` returned by the authorization server; included in the redirect URL.
    pub request_uri: CowStr<'s>,

    /// Full URL of the authorization server's token endpoint.
    pub authserver_token_endpoint: CowStr<'s>,

    /// Full URL of the authorization server's revocation endpoint, if advertised.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub authserver_revocation_endpoint: Option<CowStr<'s>>,

    /// The PKCE code verifier whose SHA-256 hash was sent as the code challenge; required
    /// at the token exchange step to prove the initiator of the auth request.
    pub pkce_verifier: CowStr<'s>,

    /// DPoP key and any authserver nonce observed during the PAR request.
    #[serde(flatten)]
    pub dpop_data: DpopReqData<'s>,
}

impl IntoStatic for AuthRequestData<'_> {
    type Output = AuthRequestData<'static>;
    fn into_static(self) -> AuthRequestData<'static> {
        AuthRequestData {
            request_uri: self.request_uri.into_static(),
            authserver_token_endpoint: self.authserver_token_endpoint.into_static(),
            authserver_revocation_endpoint: self
                .authserver_revocation_endpoint
                .map(|s| s.into_static()),
            pkce_verifier: self.pkce_verifier.into_static(),
            dpop_data: self.dpop_data.into_static(),
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
pub struct DpopReqData<'s> {
    /// The private JWK generated fresh for this authorization request and session.
    pub dpop_key: Key,
    /// DPoP nonce received from the authorization server during the PAR exchange, if any.
    #[serde(borrow)]
    pub dpop_authserver_nonce: Option<CowStr<'s>>,
}

impl IntoStatic for DpopReqData<'_> {
    type Output = DpopReqData<'static>;
    fn into_static(self) -> DpopReqData<'static> {
        DpopReqData {
            dpop_key: self.dpop_key,
            dpop_authserver_nonce: self.dpop_authserver_nonce.into_static(),
        }
    }
}

impl DpopDataSource for DpopReqData<'_> {
    fn key(&self) -> &Key {
        &self.dpop_key
    }
    fn authserver_nonce(&self) -> Option<CowStr<'_>> {
        self.dpop_authserver_nonce.clone()
    }

    fn host_nonce(&self) -> Option<CowStr<'_>> {
        None
    }

    fn set_authserver_nonce(&mut self, nonce: CowStr<'_>) {
        self.dpop_authserver_nonce = Some(nonce.into_static());
    }

    fn set_host_nonce(&mut self, _nonce: CowStr<'_>) {}
}

/// Static configuration for an OAuth client: the signing keyset and registered client metadata.
///
/// `ClientData` is constructed once at startup and shared (via `Arc`) across all sessions
/// managed by the same [`crate::client::OAuthClient`].
#[derive(Clone, Debug)]
pub struct ClientData<'s> {
    /// Optional private key set used for `private_key_jwt` client authentication.
    /// When `None`, the `none` authentication method is used instead.
    pub keyset: Option<Keyset>,
    /// AT Protocol-specific client registration metadata (redirect URIs, scopes, etc.).
    pub config: AtprotoClientMetadata<'s>,
}

impl<'s> IntoStatic for ClientData<'s> {
    type Output = ClientData<'static>;
    fn into_static(self) -> ClientData<'static> {
        ClientData {
            keyset: self.keyset,
            config: self.config.into_static(),
        }
    }
}

impl<'s> ClientData<'s> {
    /// Create `ClientData` with an optional signing keyset and the given client metadata.
    pub fn new(keyset: Option<Keyset>, config: AtprotoClientMetadata<'s>) -> Self {
        Self { keyset, config }
    }

    /// Create `ClientData` without a signing keyset, relying on the `none` auth method.
    ///
    /// Suitable for public clients (e.g., single-page applications or native apps) that
    /// cannot securely store a private key.
    pub fn new_public(config: AtprotoClientMetadata<'s>) -> Self {
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
pub struct ClientSession<'s> {
    /// Optional signing keyset, forwarded from [`ClientData`].
    pub keyset: Option<Keyset>,
    /// Client registration metadata, forwarded from [`ClientData`].
    pub config: AtprotoClientMetadata<'s>,
    /// The session state for the authenticated account.
    pub session_data: ClientSessionData<'s>,
}

impl<'s> ClientSession<'s> {
    /// Construct a `ClientSession` from a [`ClientData`] and an active session.
    pub fn new(
        ClientData { keyset, config }: ClientData<'s>,
        session_data: ClientSessionData<'s>,
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
    ) -> Result<OAuthMetadata, Error> {
        Ok(OAuthMetadata {
            server_metadata: client
                .get_authorization_server_metadata(&self.session_data.authserver_url)
                .await
                .map_err(|e| Error::ServerAgent(crate::request::RequestError::resolver(e)))?,
            client_metadata: atproto_client_metadata(self.config.clone(), &self.keyset)
                .unwrap()
                .into_static(),
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
pub struct SessionRegistry<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Backing store for persisting session data across process restarts.
    pub store: Arc<S>,
    /// Shared resolver used to fetch authorization server metadata during refresh.
    pub client: Arc<T>,
    /// Static client configuration (keyset and registration metadata).
    pub client_data: ClientData<'static>,
    /// Per-`(DID, session_id)` mutex that serializes concurrent refresh attempts.
    pending: DashMap<SmolStr, Arc<Mutex<()>>>,
}

impl<T, S> SessionRegistry<T, S>
where
    S: ClientAuthStore,
    T: OAuthResolver,
{
    /// Create a new registry, taking ownership of the store.
    pub fn new(store: S, client: Arc<T>, client_data: ClientData<'static>) -> Self {
        let store = Arc::new(store);
        Self {
            store: Arc::clone(&store),
            client,
            client_data,
            pending: DashMap::new(),
        }
    }

    /// Create a new registry from an already-`Arc`-wrapped store.
    ///
    /// Use this variant when the store needs to be accessed from outside the registry,
    /// for example to expose session listing or administration functionality.
    pub fn new_shared(store: Arc<S>, client: Arc<T>, client_data: ClientData<'static>) -> Self {
        Self {
            store,
            client,
            client_data,
            pending: DashMap::new(),
        }
    }
}

impl<T, S> SessionRegistry<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    async fn get_refreshed(
        &self,
        did: &Did<'_>,
        session_id: &str,
    ) -> Result<ClientSessionData<'_>, Error> {
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
    pub async fn get(
        &self,
        did: &Did<'_>,
        session_id: &str,
        refresh: bool,
    ) -> Result<ClientSessionData<'_>, Error> {
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
    pub async fn set(&self, value: ClientSessionData<'_>) -> Result<(), Error> {
        self.store.upsert_session(value).await?;
        Ok(())
    }
    /// Delete a session from the backing store.
    pub async fn del(&self, did: &Did<'_>, session_id: &str) -> Result<(), Error> {
        self.store.delete_session(did, session_id).await?;
        Ok(())
    }
}
