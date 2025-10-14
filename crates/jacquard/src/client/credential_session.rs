use std::sync::Arc;

use jacquard_api::com_atproto::server::{
    create_session::CreateSession, refresh_session::RefreshSession,
};
use jacquard_common::{
    AuthorizationToken, CowStr, IntoStatic,
    error::{AuthError, ClientError, TransportError, XrpcResult},
    http_client::HttpClient,
    session::SessionStore,
    types::{did::Did, string::Handle},
    xrpc::{CallOptions, Response, XrpcClient, XrpcError, XrpcExt, XrpcRequest, XrpcResp},
};
use tokio::sync::RwLock;
use url::Url;

use crate::client::AtpSession;
use jacquard_identity::resolver::IdentityResolver;
use std::any::Any;

/// Storage key for app‑password sessions: `(account DID, session id)`.
pub type SessionKey = (Did<'static>, CowStr<'static>);

/// Stateful client for app‑password based sessions.
///
/// - Persists sessions via a pluggable `SessionStore`.
/// - Automatically refreshes on token expiry.
/// - Tracks a base endpoint, defaulting to the public appview until login/restore.
pub struct CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    store: Arc<S>,
    client: Arc<T>,
    /// Default call options applied to each request (auth/headers/labelers).
    pub options: RwLock<CallOptions<'static>>,
    /// Active session key, if any.
    pub key: RwLock<Option<SessionKey>>,
    /// Current base endpoint (PDS); defaults to public appview when unset.
    pub endpoint: RwLock<Option<Url>>,
}

impl<S, T> CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    /// Create a new credential session using the given store and client.
    pub fn new(store: Arc<S>, client: Arc<T>) -> Self {
        Self {
            store,
            client,
            options: RwLock::new(CallOptions::default()),
            key: RwLock::new(None),
            endpoint: RwLock::new(None),
        }
    }
}

impl<S, T> CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    /// Return a copy configured with the provided default call options.
    pub fn with_options(self, options: CallOptions<'_>) -> Self {
        Self {
            client: self.client,
            store: self.store,
            options: RwLock::new(options.into_static()),
            key: self.key,
            endpoint: self.endpoint,
        }
    }

    /// Replace default call options.
    pub async fn set_options(&self, options: CallOptions<'_>) {
        *self.options.write().await = options.into_static();
    }

    /// Get the active session key (account DID and session id), if any.
    pub async fn session_info(&self) -> Option<SessionKey> {
        self.key.read().await.clone()
    }

    /// Current base endpoint. Defaults to the public appview when unset.
    pub async fn endpoint(&self) -> Url {
        self.endpoint.read().await.clone().unwrap_or(
            Url::parse("https://public.bsky.app").expect("public appview should be valid url"),
        )
    }

    /// Override the current base endpoint.
    pub async fn set_endpoint(&self, endpoint: Url) {
        *self.endpoint.write().await = Some(endpoint);
    }

    /// Current access token (Bearer), if logged in.
    pub async fn access_token(&self) -> Option<AuthorizationToken<'_>> {
        let key = self.key.read().await.clone()?;
        let session = self.store.get(&key).await;
        session.map(|session| AuthorizationToken::Bearer(session.access_jwt))
    }

    /// Current refresh token (Bearer), if logged in.
    pub async fn refresh_token(&self) -> Option<AuthorizationToken<'_>> {
        let key = self.key.read().await.clone()?;
        let session = self.store.get(&key).await;
        session.map(|session| AuthorizationToken::Bearer(session.refresh_jwt))
    }
}

impl<S, T> CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
    T: HttpClient,
{
    /// Refresh the active session by calling `com.atproto.server.refreshSession`.
    pub async fn refresh(&self) -> Result<AuthorizationToken<'_>, ClientError> {
        let key = self
            .key
            .read()
            .await
            .clone()
            .ok_or(ClientError::Auth(AuthError::NotAuthenticated))?;
        let session = self.store.get(&key).await;
        let endpoint = self.endpoint().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = session.map(|s| AuthorizationToken::Bearer(s.refresh_jwt));
        let response = self
            .client
            .xrpc(endpoint)
            .with_options(opts)
            .send(&RefreshSession)
            .await?;
        let refresh = response
            .parse()
            .map_err(|_| ClientError::Auth(AuthError::RefreshFailed))?;

        let new_session: AtpSession = refresh.into();
        let token = AuthorizationToken::Bearer(new_session.access_jwt.clone());
        self.store
            .set(key, new_session)
            .await
            .map_err(|_| ClientError::Auth(AuthError::RefreshFailed))?;

        Ok(token)
    }
}

impl<S, T> CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
    T: HttpClient + IdentityResolver + XrpcExt + Sync + Send,
{
    /// Resolve the user's PDS and create an app-password session.
    ///
    /// - `identifier`: handle (preferred), DID, or `https://` PDS base URL.
    /// - `session_id`: optional session label; defaults to "session".
    /// - Persists and activates the session, and updates the base endpoint to the user's PDS.
    pub async fn login(
        &self,
        identifier: CowStr<'_>,
        password: CowStr<'_>,
        session_id: Option<CowStr<'_>>,
        allow_takendown: Option<bool>,
        auth_factor_token: Option<CowStr<'_>>,
    ) -> Result<AtpSession, ClientError>
    where
        S: Any + 'static,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::info_span!("credential_session_login", identifier = %identifier).entered();

        // Resolve PDS base
        let pds = if identifier.as_ref().starts_with("http://")
            || identifier.as_ref().starts_with("https://")
        {
            Url::parse(identifier.as_ref()).map_err(|e| {
                ClientError::Transport(TransportError::InvalidRequest(e.to_string()))
            })?
        } else if identifier.as_ref().starts_with("did:") {
            let did = Did::new(identifier.as_ref()).map_err(|e| {
                ClientError::Transport(TransportError::InvalidRequest(format!(
                    "invalid did: {:?}",
                    e
                )))
            })?;
            let resp = self
                .client
                .resolve_did_doc(&did)
                .await
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
            resp.into_owned()
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?
                .pds_endpoint()
                .ok_or_else(|| {
                    ClientError::Transport(TransportError::InvalidRequest(
                        "missing PDS endpoint".into(),
                    ))
                })?
        } else {
            // treat as handle
            let handle =
                jacquard_common::types::string::Handle::new(identifier.as_ref()).map_err(|e| {
                    ClientError::Transport(TransportError::InvalidRequest(format!(
                        "invalid handle: {:?}",
                        e
                    )))
                })?;
            let did = self
                .client
                .resolve_handle(&handle)
                .await
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
            let resp = self
                .client
                .resolve_did_doc(&did)
                .await
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
            resp.into_owned()
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?
                .pds_endpoint()
                .ok_or_else(|| {
                    ClientError::Transport(TransportError::InvalidRequest(
                        "missing PDS endpoint".into(),
                    ))
                })?
        };

        // Build and send createSession
        use std::collections::BTreeMap;
        let req = CreateSession {
            allow_takendown,
            auth_factor_token,
            identifier: identifier.clone().into_static(),
            password: password.into_static(),
            extra_data: BTreeMap::new(),
        };

        let resp = self
            .client
            .xrpc(pds.clone())
            .with_options(self.options.read().await.clone())
            .send(&req)
            .await?;
        let out = resp
            .parse()
            .map_err(|_| ClientError::Auth(AuthError::NotAuthenticated))?;
        let session = AtpSession::from(out);

        let sid = session_id.unwrap_or_else(|| CowStr::new_static("session"));
        let key = (session.did.clone(), sid.into_static());
        self.store
            .set(key.clone(), session.clone())
            .await
            .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
        // If using FileAuthStore, persist PDS for faster resume
        if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            let _ = file_store.set_atp_pds(&key, &pds);
        }
        // Activate
        *self.key.write().await = Some(key);
        *self.endpoint.write().await = Some(pds);

        Ok(session)
    }

    /// Restore a previously persisted app-password session and set base endpoint.
    pub async fn restore(&self, did: Did<'_>, session_id: CowStr<'_>) -> Result<(), ClientError>
    where
        S: Any + 'static,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::info_span!("credential_session_restore", did = %did, session_id = %session_id).entered();

        let key = (did.clone().into_static(), session_id.clone().into_static());
        let Some(sess) = self.store.get(&key).await else {
            return Err(ClientError::Auth(AuthError::NotAuthenticated));
        };
        // Try to read cached PDS; otherwise resolve via DID
        let pds = if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            file_store.get_atp_pds(&key).ok().flatten().or_else(|| None)
        } else {
            None
        }
        .unwrap_or({
            let resp = self
                .client
                .resolve_did_doc(&did)
                .await
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
            resp.into_owned()
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?
                .pds_endpoint()
                .ok_or_else(|| {
                    ClientError::Transport(TransportError::InvalidRequest(
                        "missing PDS endpoint".into(),
                    ))
                })?
        });

        // Activate
        *self.key.write().await = Some(key.clone());
        *self.endpoint.write().await = Some(pds);
        // ensure store has the session (no-op if it existed)
        self.store
            .set((sess.did.clone(), session_id.into_static()), sess)
            .await
            .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
        if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            let _ = file_store.set_atp_pds(&key, &self.endpoint().await);
        }
        Ok(())
    }

    /// Switch to a different stored session (and refresh endpoint/PDS).
    pub async fn switch_session(
        &self,
        did: Did<'_>,
        session_id: CowStr<'_>,
    ) -> Result<(), ClientError>
    where
        S: Any + 'static,
    {
        let key = (did.clone().into_static(), session_id.into_static());
        if self.store.get(&key).await.is_none() {
            return Err(ClientError::Auth(AuthError::NotAuthenticated));
        }
        // Endpoint from store if cached, else resolve
        let pds = if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            file_store.get_atp_pds(&key).ok().flatten().or_else(|| None)
        } else {
            None
        }
        .unwrap_or({
            let resp = self
                .client
                .resolve_did_doc(&did)
                .await
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
            resp.into_owned()
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?
                .pds_endpoint()
                .ok_or_else(|| {
                    ClientError::Transport(TransportError::InvalidRequest(
                        "missing PDS endpoint".into(),
                    ))
                })?
        });
        *self.key.write().await = Some(key.clone());
        *self.endpoint.write().await = Some(pds);
        if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            let _ = file_store.set_atp_pds(&key, &self.endpoint().await);
        }
        Ok(())
    }

    /// Clear and delete the current session from the store.
    pub async fn logout(&self) -> Result<(), ClientError> {
        let Some(key) = self.key.read().await.clone() else {
            return Ok(());
        };
        self.store
            .del(&key)
            .await
            .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))?;
        *self.key.write().await = None;
        Ok(())
    }
}

impl<S, T> HttpClient for CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + XrpcExt + Send + Sync + 'static,
{
    type Error = T::Error;

    async fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<Vec<u8>>, Self::Error> {
        self.client.send_http(request).await
    }
}

impl<S, T> XrpcClient for CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + XrpcExt + Send + Sync + 'static,
{
    fn base_uri(&self) -> Url {
        // base_uri is a synchronous trait method; avoid `.await` here.
        // Under Tokio, use `block_in_place` to make a blocking RwLock read safe.
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| {
                self.endpoint.blocking_read().clone().unwrap_or(
                    Url::parse("https://public.bsky.app")
                        .expect("public appview should be valid url"),
                )
            })
        } else {
            self.endpoint.blocking_read().clone().unwrap_or(
                Url::parse("https://public.bsky.app").expect("public appview should be valid url"),
            )
        }
    }

    async fn send<R>(&self, request: R) -> XrpcResult<Response<<R as XrpcRequest>::Response>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let base_uri = self.base_uri();
        let auth = self.access_token().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = auth;
        let resp = self
            .client
            .xrpc(base_uri.clone())
            .with_options(opts.clone())
            .send(&request)
            .await;

        if is_expired(&resp) {
            let auth = self.refresh().await?;
            opts.auth = Some(auth);
            self.client
                .xrpc(base_uri)
                .with_options(opts)
                .send(&request)
                .await
        } else {
            resp
        }
    }
}

#[inline]
fn is_expired<R: XrpcResp>(response: &XrpcResult<Response<R>>) -> bool {
    match response {
        Err(ClientError::Auth(AuthError::TokenExpired)) => true,
        Ok(resp) => match resp.parse() {
            Err(XrpcError::Auth(AuthError::TokenExpired)) => true,
            _ => false,
        },
        _ => false,
    }
}

impl<S, T> IdentityResolver for CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + IdentityResolver + Send + Sync + 'static,
{
    fn options(&self) -> &jacquard_identity::resolver::ResolverOptions {
        self.client.options()
    }

    fn resolve_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<Output = Result<Did<'static>, jacquard_identity::resolver::IdentityError>>
    {
        async { self.client.resolve_handle(handle).await }
    }

    fn resolve_did_doc(
        &self,
        did: &Did<'_>,
    ) -> impl Future<
        Output = Result<
            jacquard_identity::resolver::DidDocResponse,
            jacquard_identity::resolver::IdentityError,
        >,
    > {
        async { self.client.resolve_did_doc(did).await }
    }
}
