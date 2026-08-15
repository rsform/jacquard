use std::sync::Arc;

use jacquard_common::xrpc::atproto::{CreateSession, RefreshSession};
use jacquard_common::{
    AuthorizationToken, IntoStatic,
    bos::BosStr,
    deps::fluent_uri::Uri,
    error::{AuthError, ClientError, XrpcResult},
    http_client::HttpClient,
    session::{MemorySessionStore, SessionHint, SessionSelector, SessionStore},
    types::{did::Did, string::Handle},
    xrpc::{CallOptions, Response, XrpcClient, XrpcExt, XrpcRequest, XrpcResp, XrpcResponse},
};
#[cfg(feature = "streaming")]
use serde::Serialize;
use smol_str::{SmolStr, ToSmolStr};
use tokio::sync::RwLock;

use crate::client::AtpSession;
#[cfg(feature = "websocket")]
use jacquard_common::xrpc::XrpcSubscription;
#[cfg(feature = "websocket")]
use jacquard_common::{
    CowStr,
    websocket::{WebSocketClient, WebSocketConnection},
};
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};

pub use jacquard_common::session::SessionKey;

/// App-password session lookup result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialSessionMatch {
    /// Matched session key.
    pub key: SessionKey,
    /// Stored app-password session for the matched key.
    pub session: AtpSession,
}

/// Result of trying to resume an app-password session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialResumeResult {
    /// A stored session was found and activated.
    Resumed(AtpSession),
    /// No stored session matched; login credentials are required.
    LoginRequired(CredentialLoginChallenge),
}

/// Login identity details derived from a failed resume hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialLoginChallenge {
    /// Login identifier known from the hint, if any.
    pub identifier: Option<SmolStr>,
    /// Session id known from the hint, if any.
    pub session_id: Option<SmolStr>,
}

/// Options for hint/challenge-based app-password login helpers.
#[derive(Debug, Clone)]
pub struct CredentialLoginOptions {
    /// App-password or account password.
    pub password: SmolStr,
    /// Login identifier override, required when the challenge has no identifier.
    pub identifier: Option<SmolStr>,
    /// Whether taken-down accounts are allowed.
    pub allow_takendown: Option<bool>,
    /// Optional auth factor token.
    pub auth_factor_token: Option<SmolStr>,
    /// Explicit PDS/entryway endpoint to use for login.
    pub pds: Option<Uri<String>>,
}

/// Resolver-backed app-password session selector.
///
/// This adapter returns richer credential-session match data while keeping selection pluggable:
/// database-backed stores can provide their own [`SessionSelector`] implementation with more
/// efficient indexed lookup.
pub struct CredentialSessionSelector<'a, S, R> {
    store: &'a S,
    resolver: &'a R,
}

impl<'a, S, R> CredentialSessionSelector<'a, S, R> {
    /// Create a selector over an app-password session store and identity resolver.
    pub fn new(store: &'a S, resolver: &'a R) -> Self {
        Self { store, resolver }
    }
}

impl<S, R> SessionSelector<CredentialSessionMatch> for CredentialSessionSelector<'_, S, R>
where
    S: SessionStore<SessionKey, AtpSession>
        + SessionSelector<CredentialSessionMatch, Error = ClientError>
        + Sync,
    R: IdentityResolver + Sync,
{
    type Error = ClientError;

    async fn select_session<Str: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<Str>,
    ) -> Result<Option<CredentialSessionMatch>, Self::Error> {
        if let Some(matched) = self.store.select_session(hint).await? {
            return Ok(Some(matched));
        }

        let SessionHint::Handle(handle) = hint else {
            return Ok(None);
        };

        let did = self.resolver.resolve_handle(handle).await?;
        self.store.select_session(&SessionHint::Did(did)).await
    }
}

/// Resolve a session hint against an app-password [`SessionStore`].
///
/// This is a convenience wrapper around [`CredentialSessionSelector`].
pub async fn resolve_credential_session_hint<S, R, Str>(
    store: &S,
    resolver: &R,
    hint: &SessionHint<Str>,
) -> Result<Option<CredentialSessionMatch>, ClientError>
where
    S: SessionStore<SessionKey, AtpSession>
        + SessionSelector<CredentialSessionMatch, Error = ClientError>
        + Sync,
    R: IdentityResolver + Sync,
    Str: BosStr + Send + Sync,
{
    CredentialSessionSelector::new(store, resolver)
        .select_session(hint)
        .await
}

async fn match_credential_session_key<S>(
    store: &S,
    key: SessionKey,
) -> Result<Option<CredentialSessionMatch>, ClientError>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    Ok(store
        .get(&key)
        .await
        .map(|session| CredentialSessionMatch { key, session }))
}

impl SessionSelector<CredentialSessionMatch> for MemorySessionStore<SessionKey, AtpSession> {
    type Error = ClientError;

    async fn select_session<Str: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<Str>,
    ) -> Result<Option<CredentialSessionMatch>, Self::Error> {
        match hint {
            SessionHint::Any => {
                let Some(key) = self.list_keys().await?.into_iter().next() else {
                    return Ok(None);
                };
                match_credential_session_key(self, key).await
            }
            SessionHint::Did(did) => {
                for key in self.list_keys().await? {
                    if key.did.as_str() == did.as_ref() {
                        if let Some(matched) = match_credential_session_key(self, key).await? {
                            return Ok(Some(matched));
                        }
                    }
                }
                Ok(None)
            }
            SessionHint::Handle(handle) => {
                for key in self.list_keys().await? {
                    if let Some(session) = self.get(&key).await {
                        if session.handle.as_str() == handle.as_ref() {
                            return Ok(Some(CredentialSessionMatch { key, session }));
                        }
                    }
                }
                Ok(None)
            }
            SessionHint::Key(key) => match_credential_session_key(self, key.clone()).await,
            SessionHint::Identifier(_) => Ok(None),
        }
    }
}

fn credential_challenge_from_hint<S: BosStr>(hint: &SessionHint<S>) -> CredentialLoginChallenge {
    match hint {
        SessionHint::Any => CredentialLoginChallenge {
            identifier: None,
            session_id: None,
        },
        SessionHint::Did(did) => CredentialLoginChallenge {
            identifier: Some(did.to_smolstr()),
            session_id: None,
        },
        SessionHint::Handle(handle) => CredentialLoginChallenge {
            identifier: Some(handle.to_smolstr()),
            session_id: None,
        },
        SessionHint::Key(key) => CredentialLoginChallenge {
            identifier: Some(key.did.to_smolstr()),
            session_id: Some(key.session_id.clone()),
        },
        SessionHint::Identifier(identifier) => CredentialLoginChallenge {
            identifier: Some(identifier.as_ref().to_smolstr()),
            session_id: None,
        },
    }
}

/// Stateful client for app‑password based sessions.
///
/// - Persists sessions via a pluggable `SessionStore`.
/// - Automatically refreshes on token expiry.
/// - Tracks a base endpoint, defaulting to the public appview until login/restore.
/// - Optional WebSocket client for subscription support.
pub struct CredentialSession<S, T, W = ()>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    store: Arc<S>,
    client: Arc<T>,
    ws_client: W,
    /// Default call options applied to each request (auth/headers/labelers).
    pub options: RwLock<CallOptions>,
    /// Active session key, if any.
    pub key: RwLock<Option<SessionKey>>,
    /// Current base endpoint (PDS); defaults to public appview when unset.
    pub endpoint: RwLock<Option<Uri<String>>>,
}

impl<S, T> CredentialSession<S, T, ()>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    /// Create a new credential session using the given store and client (no WebSocket support).
    pub fn new(store: Arc<S>, client: Arc<T>) -> Self {
        Self {
            store,
            client,
            ws_client: (),
            options: RwLock::new(CallOptions::default()),
            key: RwLock::new(None),
            endpoint: RwLock::new(None),
        }
    }
}

impl<S, T, W> CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    /// Create a new credential session with WebSocket client support.
    pub fn new_with_ws(store: Arc<S>, client: Arc<T>, ws_client: W) -> Self {
        Self {
            store,
            client,
            ws_client,
            options: RwLock::new(CallOptions::default()),
            key: RwLock::new(None),
            endpoint: RwLock::new(None),
        }
    }

    /// Get a reference to the WebSocket client.
    pub fn ws_client(&self) -> &W {
        &self.ws_client
    }

    /// Return a copy configured with the provided default call options.
    pub fn with_options(self, options: CallOptions) -> Self {
        Self {
            client: self.client,
            store: self.store,
            ws_client: self.ws_client,
            options: RwLock::new(options.into_static()),
            key: self.key,
            endpoint: self.endpoint,
        }
    }

    /// Replace default call options.
    pub async fn set_options(&self, options: CallOptions) {
        *self.options.write().await = options.into_static();
    }

    /// Get the active session key (account DID and session id), if any.
    pub async fn session_info(&self) -> Option<SessionKey> {
        self.key.read().await.clone()
    }

    /// Current base endpoint. Defaults to the public appview when unset.
    pub async fn endpoint(&self) -> Uri<String> {
        self.endpoint.read().await.clone().unwrap_or_else(|| {
            Uri::parse("https://public.api.bsky.app")
                .expect("hardcoded URI is valid")
                .to_owned()
        })
    }

    /// Override the current base endpoint.
    pub async fn set_endpoint(&self, uri: Uri<String>) {
        let normalized = jacquard_common::xrpc::normalize_base_uri(uri);
        *self.endpoint.write().await = Some(normalized);
    }

    /// Current access token (Bearer), if logged in.
    pub async fn access_token(&self) -> Option<AuthorizationToken> {
        let key = self.key.read().await.clone()?;
        let session = self.store.get(&key).await;
        session.map(|session| AuthorizationToken::Bearer(session.access_jwt))
    }

    /// Current refresh token (Bearer), if logged in.
    pub async fn refresh_token(&self) -> Option<AuthorizationToken> {
        let key = self.key.read().await.clone()?;
        let session = self.store.get(&key).await;
        session.map(|session| AuthorizationToken::Bearer(session.refresh_jwt))
    }
}

impl<S, T, W> CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession>,
    T: HttpClient,
{
    /// Refresh the active session by calling `com.atproto.server.refreshSession`.
    pub async fn refresh(&self) -> std::result::Result<AuthorizationToken, ClientError> {
        let key = self
            .key
            .read()
            .await
            .clone()
            .ok_or_else(|| ClientError::auth(AuthError::NotAuthenticated))?;
        let session = self.store.get(&key).await;
        let endpoint = self.endpoint().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = session
            .as_ref()
            .map(|s| AuthorizationToken::Bearer(s.refresh_jwt.clone()));
        let response = self
            .client
            .xrpc(endpoint.borrow())
            .with_options(opts)
            .send(&RefreshSession)
            .await?;
        let refresh = response.parse().map_err(|_| {
            ClientError::auth(AuthError::RefreshFailed)
                .with_help("ensure refresh token is valid and not expired")
                .with_url("com.atproto.server.refreshSession")
        })?;

        let mut new_session = session.unwrap_or_else(|| AtpSession::from(refresh.clone()));
        new_session.merge_refresh(refresh);
        let token = AuthorizationToken::Bearer(new_session.access_jwt.clone());
        self.store.set(key, new_session).await.map_err(|e| {
            ClientError::from(e).with_context("failed to persist refreshed session to store")
        })?;

        Ok(token)
    }
}

impl<S, T, W> CredentialSession<S, T, W>
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
        identifier: &str,
        password: &str,
        session_id: Option<&str>,
        allow_takendown: Option<bool>,
        auth_factor_token: Option<&str>,
        pds: Option<Uri<String>>,
    ) -> std::result::Result<AtpSession, ClientError> {
        #[cfg(feature = "tracing")]
        let _span =
            tracing::info_span!("credential_session_login", identifier = %identifier).entered();

        // Resolve PDS base
        let pds = if let Some(pds) = pds {
            pds
        } else if identifier.starts_with("http://") || identifier.starts_with("https://") {
            Uri::parse(identifier)
                .map_err(|e| {
                    ClientError::from(e)
                        .with_help("identifier should be a valid https:// URL, handle, or DID")
                })?
                .to_owned()
        } else if identifier.starts_with("did:") {
            let did = Did::new(identifier).map_err(|e| {
                ClientError::invalid_request(format!("invalid did: {:?}", e))
                    .with_help("DID format should be did:method:identifier (e.g., did:plc:abc123)")
            })?;
            let resp = self.client.resolve_did_doc(&did).await.map_err(|e| {
                ClientError::from(e).with_context("DID document resolution failed during login")
            })?;
            resp.into_owned()?
                .pds_endpoint()
                .map(|u| u.to_owned())
                .ok_or_else(|| {
                    ClientError::invalid_request("missing PDS endpoint")
                        .with_help("DID document must include a PDS service endpoint")
                })?
        } else if identifier.contains('@') && !identifier.starts_with('@') {
            // we're going to assume its an email
            pds.ok_or_else(|| {
                ClientError::invalid_request("missing PDS endpoint")
                    .with_help("When logging in with email, we need your PDS")
            })?
        } else {
            // treat as handle
            let handle = jacquard_common::types::string::Handle::new(identifier).map_err(|e| {
                ClientError::invalid_request(format!("invalid handle: {:?}", e))
                    .with_help("handle format should be domain.tld (e.g., alice.bsky.social)")
            })?;
            let did = self.client.resolve_handle(&handle).await.map_err(|e| {
                ClientError::from(e).with_context("handle resolution failed during login")
            })?;
            let resp = self.client.resolve_did_doc(&did).await.map_err(|e| {
                ClientError::from(e).with_context("DID document resolution failed during login")
            })?;
            resp.into_owned()?
                .pds_endpoint()
                .map(|u| u.to_owned())
                .ok_or_else(|| {
                    ClientError::invalid_request("missing PDS endpoint")
                        .with_help("DID document must include a PDS service endpoint")
                })?
        };

        // Build and send createSession
        let req = CreateSession {
            allow_takendown,
            auth_factor_token: auth_factor_token.map(SmolStr::from),
            identifier: SmolStr::from(identifier),
            password: SmolStr::from(password),
        };

        let resp = self
            .client
            .xrpc(pds.borrow())
            .with_options(self.options.read().await.clone())
            .send(&req)
            .await?;
        let out = resp.parse().map_err(|_| {
            ClientError::auth(AuthError::NotAuthenticated)
                .with_help("check identifier and password are correct")
                .with_url("com.atproto.server.createSession")
        })?;
        let mut session = AtpSession::from(out);
        if session.pds.is_none() {
            session.pds = Some(jacquard_common::xrpc::normalize_base_uri(pds.clone()));
        }

        let sid = SmolStr::from(session_id.unwrap_or("session"));
        let key = SessionKey::new(session.did.clone().convert::<SmolStr>(), sid);
        self.store
            .set(key.clone(), session.clone())
            .await
            .map_err(|e| ClientError::from(e).with_context("failed to persist session to store"))?;
        // Activate
        *self.key.write().await = Some(key);
        let pds_uri = jacquard_common::xrpc::normalize_base_uri(session.pds.clone().unwrap_or(pds));
        *self.endpoint.write().await = Some(pds_uri);

        Ok(session)
    }

    async fn activate_session(
        &self,
        key: SessionKey,
        mut session: AtpSession,
    ) -> std::result::Result<AtpSession, ClientError> {
        let pds = if let Some(pds) = session.pds.clone() {
            pds
        } else {
            let resp = self.client.resolve_did_doc(&session.did).await?;
            let pds = resp
                .into_owned()?
                .pds_endpoint()
                .map(|u| u.to_owned())
                .ok_or_else(|| {
                    ClientError::invalid_request("missing PDS endpoint")
                        .with_help("DID document must include a PDS service endpoint")
                })?;
            session.pds = Some(jacquard_common::xrpc::normalize_base_uri(pds));
            self.store
                .set(key.clone(), session.clone())
                .await
                .map_err(|e| {
                    ClientError::from(e).with_context("failed to persist session PDS to store")
                })?;
            session.pds.clone().expect("pds just set")
        };

        *self.key.write().await = Some(key);
        *self.endpoint.write().await = Some(jacquard_common::xrpc::normalize_base_uri(pds));
        Ok(session)
    }

    /// Try to resume a stored app-password session for the given hint.
    pub async fn resume<Str: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<Str>,
    ) -> Result<CredentialResumeResult, ClientError>
    where
        S: SessionSelector<CredentialSessionMatch, Error = ClientError>,
    {
        match CredentialSessionSelector::new(self.store.as_ref(), self.client.as_ref())
            .select_session(hint)
            .await?
        {
            Some(matched) => {
                let session = self.activate_session(matched.key, matched.session).await?;
                Ok(CredentialResumeResult::Resumed(session))
            }
            None => Ok(CredentialResumeResult::LoginRequired(
                credential_challenge_from_hint(hint),
            )),
        }
    }

    /// Login using identity details from a resume challenge.
    pub async fn login_from_challenge(
        &self,
        challenge: CredentialLoginChallenge,
        options: CredentialLoginOptions,
    ) -> Result<AtpSession, ClientError> {
        let identifier = challenge
            .identifier
            .as_ref()
            .map(|s| s.as_str())
            .or(options.identifier.as_deref())
            .ok_or_else(|| {
                ClientError::invalid_request("missing login identifier").with_help(
                    "provide CredentialLoginOptions::identifier for an Any resume challenge",
                )
            })?;
        self.login(
            identifier,
            &options.password,
            challenge.session_id.as_deref(),
            options.allow_takendown,
            options.auth_factor_token.as_deref(),
            options.pds,
        )
        .await
    }

    /// Login using identity details derived from a session hint.
    pub async fn login_with_hint<Str: BosStr>(
        &self,
        hint: &SessionHint<Str>,
        options: CredentialLoginOptions,
    ) -> Result<AtpSession, ClientError> {
        self.login_from_challenge(credential_challenge_from_hint(hint), options)
            .await
    }

    /// Resume a stored session if available, otherwise login using the provided options.
    pub async fn resume_or_login<Str: BosStr + Send + Sync>(
        &self,
        hint: &SessionHint<Str>,
        options: CredentialLoginOptions,
    ) -> Result<AtpSession, ClientError>
    where
        S: SessionSelector<CredentialSessionMatch, Error = ClientError>,
    {
        match self.resume(hint).await? {
            CredentialResumeResult::Resumed(session) => Ok(session),
            CredentialResumeResult::LoginRequired(challenge) => {
                self.login_from_challenge(challenge, options).await
            }
        }
    }

    /// Restore a previously persisted app-password session and set base endpoint.
    pub async fn restore(
        &self,
        did: Did,
        session_id: &str,
    ) -> std::result::Result<(), ClientError> {
        #[cfg(feature = "tracing")]
        let _span =
            tracing::info_span!("credential_session_restore", did = %did, session_id = %session_id)
                .entered();

        let key = SessionKey::new(did, SmolStr::from(session_id));
        let Some(sess) = self.store.get(&key).await else {
            return Err(ClientError::auth(AuthError::NotAuthenticated));
        };
        self.activate_session(key, sess).await.map(|_| ())
    }

    /// Switch to a different stored session (and refresh endpoint/PDS).
    pub async fn switch_session(
        &self,
        did: Did,
        session_id: &str,
    ) -> std::result::Result<(), ClientError> {
        let key = SessionKey::new(did, SmolStr::from(session_id));
        let Some(sess) = self.store.get(&key).await else {
            return Err(ClientError::auth(AuthError::NotAuthenticated));
        };
        self.activate_session(key, sess).await.map(|_| ())
    }

    /// Clear and delete the current session from the store.
    pub async fn logout(&self) -> std::result::Result<(), ClientError> {
        let Some(key) = self.key.read().await.clone() else {
            return Ok(());
        };
        self.store.del(&key).await?;
        *self.key.write().await = None;
        Ok(())
    }
}

impl<S, T, W> HttpClient for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + XrpcExt + Send + Sync + 'static,
    W: Send + Sync,
{
    type Error = T::Error;

    async fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<Vec<u8>>, Self::Error> {
        self.client.send_http(request).await
    }
}

impl<S, T, W> XrpcClient for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + XrpcExt + Send + Sync + 'static,
    W: Send + Sync,
{
    async fn base_uri(&self) -> Uri<String> {
        self.endpoint.read().await.clone().unwrap_or_else(|| {
            Uri::parse("https://public.api.bsky.app")
                .expect("hardcoded URI is valid")
                .to_owned()
        })
    }

    async fn opts(&self) -> CallOptions {
        self.options.read().await.clone()
    }

    async fn set_opts(&self, opts: CallOptions) {
        let mut guard = self.options.write().await;
        *guard = opts.into_static();
    }

    async fn set_base_uri(&self, uri: Uri<String>) {
        let normalized = jacquard_common::xrpc::normalize_base_uri(uri);
        let mut guard = self.endpoint.write().await;
        *guard = Some(normalized);
    }

    async fn send<R>(&self, request: R) -> XrpcResult<XrpcResponse<R>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let opts = self.options.read().await.clone();
        self.send_with_opts(request, opts).await
    }

    async fn send_with_opts<R>(
        &self,
        request: R,
        mut opts: CallOptions,
    ) -> XrpcResult<XrpcResponse<R>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let base_uri = self.base_uri().await;
        let auth = self.access_token().await;
        opts.auth = auth;
        let resp = self
            .client
            .xrpc(base_uri.borrow())
            .with_options(opts.clone())
            .send(&request)
            .await;

        if is_expired(&resp) {
            let auth = self.refresh().await?;
            opts.auth = Some(auth);
            self.client
                .xrpc(base_uri.borrow())
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
        Err(e)
            if matches!(
                e.kind(),
                jacquard_common::error::ClientErrorKind::Auth(AuthError::TokenExpired)
            ) =>
        {
            true
        }
        // A 401 without WWW-Authenticate flows through as Ok(Response) with 401 status.
        // Check the status code directly to catch expired tokens signalled via body JSON.
        Ok(resp) if resp.status() == http::StatusCode::UNAUTHORIZED => true,
        _ => false,
    }
}

#[cfg(feature = "streaming")]
impl<S, T, W> jacquard_common::http_client::HttpClientExt for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + XrpcExt + jacquard_common::http_client::HttpClientExt + Send + Sync + 'static,
    W: Send + Sync,
{
    async fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<jacquard_common::stream::ByteStream>, Self::Error>
    {
        self.client.send_http_streaming(request).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn send_http_bidirectional<Str>(
        &self,
        parts: http::request::Parts,
        body: Str,
    ) -> core::result::Result<http::Response<jacquard_common::stream::ByteStream>, Self::Error>
    where
        Str: n0_future::Stream<
                Item = core::result::Result<bytes::Bytes, jacquard_common::StreamError>,
            > + Send
            + 'static,
    {
        self.client.send_http_bidirectional(parts, body).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn send_http_bidirectional<Str>(
        &self,
        parts: http::request::Parts,
        body: Str,
    ) -> core::result::Result<http::Response<jacquard_common::stream::ByteStream>, Self::Error>
    where
        Str: n0_future::Stream<
                Item = core::result::Result<bytes::Bytes, jacquard_common::StreamError>,
            > + 'static,
    {
        self.client.send_http_bidirectional(parts, body).await
    }
}

#[cfg(feature = "streaming")]
impl<S, T, W> jacquard_common::xrpc::XrpcStreamingClient for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + XrpcExt + jacquard_common::http_client::HttpClientExt + Send + Sync + 'static,
    W: Send + Sync,
{
    async fn download<R>(
        &self,
        request: R,
    ) -> core::result::Result<jacquard_common::xrpc::StreamingResponse, jacquard_common::StreamError>
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        use jacquard_common::{StreamError, xrpc::build_http_request};

        let base_uri = <Self as XrpcClient>::base_uri(self).await;
        let mut opts = self.options.read().await.clone();
        opts.auth = self.access_token().await;

        let http_request = build_http_request(&base_uri.borrow(), &request, &opts)
            .map_err(|e| StreamError::protocol(e.to_string()))?;

        let response = self
            .client
            .send_http_streaming(http_request.clone())
            .await
            .map_err(StreamError::transport)?;

        let (parts, body) = response.into_parts();
        let status = parts.status;

        // Check if expired based on status code
        if status == http::StatusCode::UNAUTHORIZED || status == http::StatusCode::BAD_REQUEST {
            // Try to refresh
            let auth = self.refresh().await.map_err(StreamError::transport)?;
            opts.auth = Some(auth);

            let http_request = build_http_request(&base_uri.borrow(), &request, &opts)
                .map_err(|e| StreamError::protocol(e.to_string()))?;

            let response = self
                .client
                .send_http_streaming(http_request)
                .await
                .map_err(StreamError::transport)?;
            let (parts, body) = response.into_parts();
            Ok(jacquard_common::xrpc::StreamingResponse::new(parts, body))
        } else {
            Ok(jacquard_common::xrpc::StreamingResponse::new(parts, body))
        }
    }

    async fn stream<Str, B>(
        &self,
        stream: jacquard_common::xrpc::streaming::XrpcProcedureSend<Str::Frame<B>>,
    ) -> core::result::Result<
        jacquard_common::xrpc::streaming::XrpcResponseStream<
            <<Str as jacquard_common::xrpc::streaming::XrpcProcedureStream>::Response as jacquard_common::xrpc::streaming::XrpcStreamResp>::Frame<B>,
        >,
        jacquard_common::StreamError,
    >
    where
        B: BosStr + 'static,
        Str: jacquard_common::xrpc::streaming::XrpcProcedureStream + 'static,
        <<Str as jacquard_common::xrpc::streaming::XrpcProcedureStream>::Response as jacquard_common::xrpc::streaming::XrpcStreamResp>::Frame<B>: jacquard_common::xrpc::streaming::XrpcStreamResp,
    {
        use jacquard_common::StreamError;
        use n0_future::TryStreamExt;

        let base_uri = self.base_uri().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = self.access_token().await;

        let mut path = String::from(base_uri.as_str().trim_end_matches('/'));
        path.push_str("/xrpc/");
        path.push_str(<Str::Request as jacquard_common::xrpc::XrpcRequest>::NSID);

        let mut builder = http::Request::post(&path);

        if let Some(token) = &opts.auth {
            use jacquard_common::AuthorizationToken;
            let hv = match token {
                AuthorizationToken::Bearer(t) => {
                    http::HeaderValue::from_str(&format!("Bearer {}", t.as_str()))
                }
                AuthorizationToken::Dpop(t) | AuthorizationToken::Delegation(t) => {
                    http::HeaderValue::from_str(&format!("DPoP {}", t.as_str()))
                }
            }
            .map_err(|e| StreamError::protocol(format!("Invalid authorization token: {}", e)))?;
            builder = builder.header(http::header::AUTHORIZATION, hv);
        }

        if let Some(proxy) = &opts.atproto_proxy {
            builder = builder.header("atproto-proxy", proxy.as_str());
        }
        if let Some(labelers) = &opts.atproto_accept_labelers {
            if !labelers.is_empty() {
                let joined = labelers
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>()
                    .join(", ");
                builder = builder.header("atproto-accept-labelers", joined);
            }
        }
        for (name, value) in &opts.extra_headers {
            builder = builder.header(name, value);
        }

        let (parts, _) = builder
            .body(())
            .map_err(|e| StreamError::protocol(e.to_string()))?
            .into_parts();

        let body_stream =
            jacquard_common::stream::ByteStream::new(Box::pin(stream.0.map_ok(|f| f.buffer)));

        // Clone the stream for potential retry
        let (body1, body2) = body_stream.tee();

        let response = self
            .client
            .send_http_bidirectional(parts.clone(), body1.into_inner())
            .await
            .map_err(StreamError::transport)?;

        let (resp_parts, resp_body) = response.into_parts();
        let status = resp_parts.status;

        // Check if expired
        if status == http::StatusCode::UNAUTHORIZED || status == http::StatusCode::BAD_REQUEST {
            // Try to refresh
            let auth = self.refresh().await.map_err(StreamError::transport)?;
            opts.auth = Some(auth);

            // Rebuild request with new auth
            let mut builder = http::Request::post(path.clone());
            if let Some(token) = &opts.auth {
                use jacquard_common::AuthorizationToken;
                let hv = match token {
                    AuthorizationToken::Bearer(t) => {
                        http::HeaderValue::from_str(&format!("Bearer {}", t.as_str()))
                    }
                    AuthorizationToken::Dpop(t) | AuthorizationToken::Delegation(t) => {
                        http::HeaderValue::from_str(&format!("DPoP {}", t.as_str()))
                    }
                }
                .map_err(|e| {
                    StreamError::protocol(format!("Invalid authorization token: {}", e))
                })?;
                builder = builder.header(http::header::AUTHORIZATION, hv);
            }
            if let Some(proxy) = &opts.atproto_proxy {
                builder = builder.header("atproto-proxy", proxy.as_str());
            }
            if let Some(labelers) = &opts.atproto_accept_labelers {
                if !labelers.is_empty() {
                    let joined = labelers
                        .iter()
                        .map(|s| s.as_ref())
                        .collect::<Vec<_>>()
                        .join(", ");
                    builder = builder.header("atproto-accept-labelers", joined);
                }
            }
            for (name, value) in &opts.extra_headers {
                builder = builder.header(name, value);
            }

            let (parts, _) = builder
                .body(())
                .map_err(|e| StreamError::protocol(e.to_string()))?
                .into_parts();

            // Retry with the second half of the cloned stream
            let response = self
                .client
                .send_http_bidirectional(parts, body2.into_inner())
                .await
                .map_err(StreamError::transport)?;
            let (resp_parts, resp_body) = response.into_parts();
            Ok(
                jacquard_common::xrpc::streaming::XrpcResponseStream::from_typed_parts::<B>(
                    resp_parts, resp_body,
                ),
            )
        } else {
            Ok(
                jacquard_common::xrpc::streaming::XrpcResponseStream::from_typed_parts::<B>(
                    resp_parts, resp_body,
                ),
            )
        }
    }
}

impl<S, T, W> IdentityResolver for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: HttpClient + IdentityResolver + Send + Sync + 'static,
    W: Send + Sync,
{
    fn options(&self) -> &ResolverOptions {
        self.client.options()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_handle<Str: BosStr + Sync>(
        &self,
        handle: &Handle<Str>,
    ) -> impl Future<Output = Result<Did, IdentityError>>
    where
        Self: Sync,
    {
        async { self.client.resolve_handle(handle).await }
    }

    #[cfg(target_arch = "wasm32")]
    fn resolve_handle<Str: BosStr + Sync>(
        &self,
        handle: &Handle<Str>,
    ) -> impl Future<Output = Result<Did, IdentityError>> {
        async { self.client.resolve_handle(handle).await }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_did_doc<Str: BosStr + Sync>(
        &self,
        did: &Did<Str>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>>
    where
        Self: Sync,
    {
        async { self.client.resolve_did_doc(did).await }
    }

    #[cfg(target_arch = "wasm32")]
    fn resolve_did_doc<Str: BosStr + Sync>(
        &self,
        did: &Did<Str>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>> {
        async { self.client.resolve_did_doc(did).await }
    }
}

#[cfg(feature = "websocket")]
impl<S, T, W> WebSocketClient for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    W: WebSocketClient + Send + Sync,
{
    type Error = W::Error;

    async fn connect(&self, uri: Uri<&str>) -> Result<WebSocketConnection, Self::Error> {
        self.ws_client.connect(uri).await
    }

    async fn connect_with_headers(
        &self,
        uri: Uri<&str>,
        headers: Vec<(CowStr<'_>, CowStr<'_>)>,
    ) -> Result<WebSocketConnection, Self::Error> {
        self.ws_client.connect_with_headers(uri, headers).await
    }
}

#[cfg(feature = "websocket")]
impl<S, T, W> jacquard_common::xrpc::SubscriptionClient for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: Send + Sync + 'static,
    W: WebSocketClient + Send + Sync,
{
    async fn base_uri(&self) -> Uri<String> {
        self.endpoint.read().await.clone().unwrap_or_else(|| {
            Uri::parse("https://public.api.bsky.app")
                .expect("hardcoded URI is valid")
                .to_owned()
        })
    }

    async fn subscription_opts(&self) -> jacquard_common::xrpc::SubscriptionOptions<'_> {
        let mut opts = jacquard_common::xrpc::SubscriptionOptions::default();
        if let Some(token) = self.access_token().await {
            let auth_value = match token {
                AuthorizationToken::Bearer(t) => format!("Bearer {}", t.as_str()),
                AuthorizationToken::Dpop(t) | AuthorizationToken::Delegation(t) => {
                    format!("DPoP {}", t.as_str())
                }
            };
            opts.headers
                .push((CowStr::from("Authorization"), CowStr::from(auth_value)));
        }
        opts
    }

    async fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> Result<jacquard_common::xrpc::SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync + serde::Serialize,
    {
        let opts = self.subscription_opts().await;
        self.subscribe_with_opts(params, opts).await
    }

    async fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: jacquard_common::xrpc::SubscriptionOptions<'_>,
    ) -> Result<jacquard_common::xrpc::SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync + serde::Serialize,
    {
        use jacquard_common::xrpc::SubscriptionExt;
        let base = self.base_uri().await;
        self.subscription(base)
            .with_options(opts)
            .subscribe(params)
            .await
    }
}
