use crate::{
    atproto::atproto_client_metadata,
    authstore::ClientAuthStore,
    dpop::DpopExt,
    error::{CallbackError, Result},
    request::{OAuthMetadata, exchange_code, par},
    resolver::OAuthResolver,
    scopes::Scope,
    session::{ClientData, ClientSessionData, DpopClientData, SessionRegistry},
    types::{AuthorizeOptions, CallbackParams},
};
use jacquard_common::{
    AuthorizationToken, CowStr, IntoStatic,
    cowstr::ToCowStr,
    deps::fluent_uri::Uri,
    error::{AuthError, ClientError, XrpcResult},
    http_client::HttpClient,
    types::{did::Did, string::Handle},
    xrpc::{
        CallOptions, Response, XrpcClient, XrpcError, XrpcExt, XrpcRequest, XrpcResp, XrpcResponse,
        build_http_request, process_response,
    },
};

#[cfg(feature = "websocket")]
use jacquard_common::websocket::{WebSocketClient, WebSocketConnection};
#[cfg(feature = "websocket")]
use jacquard_common::xrpc::XrpcSubscription;
use jacquard_identity::{
    JacquardResolver,
    resolver::{DidDocResponse, IdentityError, IdentityResolver, ResolverOptions},
};
use jose_jwk::JwkSet;
use std::{future::Future, sync::Arc};
use tokio::sync::RwLock;

/// The top-level OAuth client responsible for driving the authorization flow.
pub struct OAuthClient<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Shared session registry that mediates access to the backing auth store.
    pub registry: Arc<SessionRegistry<T, S>>,
    /// Default call options applied to every outgoing XRPC request.
    pub options: RwLock<CallOptions<'static>>,
    /// Override for the XRPC base URI; falls back to the public Bluesky AppView when `None`.
    pub endpoint: RwLock<Option<Uri<String>>>,
    /// Underlying HTTP/identity/OAuth resolver used for all network operations.
    pub client: Arc<T>,
}

impl<S: ClientAuthStore> OAuthClient<JacquardResolver, S> {
    /// Create an `OAuthClient` using the default [`JacquardResolver`] for identity and metadata resolution.
    pub fn new(store: S, client_data: ClientData<'static>) -> Self {
        let client = JacquardResolver::default();
        Self::new_from_resolver(store, client, client_data)
    }

    /// Create an OAuth client with the provided store and default localhost client metadata.
    ///
    /// This is a convenience constructor for quickly setting up an OAuth client
    /// with default localhost redirect URIs and "atproto transition:generic" scopes.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard_oauth::client::OAuthClient;
    /// # use jacquard_oauth::authstore::MemoryAuthStore;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let store = MemoryAuthStore::new();
    /// let oauth = OAuthClient::with_default_config(store);
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_default_config(store: S) -> Self {
        let client_data = ClientData {
            keyset: None,
            config: crate::atproto::AtprotoClientMetadata::default_localhost(),
        };
        Self::new(store, client_data)
    }
}

impl OAuthClient<JacquardResolver, crate::authstore::MemoryAuthStore> {
    /// Create an OAuth client with an in-memory auth store and default localhost client metadata.
    ///
    /// This is a convenience constructor for simple testing and development.
    /// The session will not persist across restarts.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard_oauth::client::OAuthClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let oauth = OAuthClient::with_memory_store();
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_memory_store() -> Self {
        Self::with_default_config(crate::authstore::MemoryAuthStore::new())
    }
}

impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Create an OAuth client from an explicit resolver instance, taking ownership of both.
    pub fn new_from_resolver(store: S, client: T, client_data: ClientData<'static>) -> Self {
        // #[cfg(feature = "tracing")]
        // tracing::info!(
        //     redirect_uris = ?client_data.config.redirect_uris,
        //     scopes = ?client_data.config.scopes,
        //     has_keyset = client_data.keyset.is_some(),
        //     "oauth client created:"
        // );

        let client = Arc::new(client);
        let registry = Arc::new(SessionRegistry::new(store, client.clone(), client_data));
        Self {
            registry,
            client,
            options: RwLock::new(CallOptions::default()),
            endpoint: RwLock::new(None),
        }
    }

    /// Create an OAuth client from already-`Arc`-wrapped store and resolver.
    pub fn new_with_shared(
        store: Arc<S>,
        client: Arc<T>,
        client_data: ClientData<'static>,
    ) -> Self {
        let registry = Arc::new(SessionRegistry::new_shared(
            store,
            client.clone(),
            client_data,
        ));
        Self {
            registry,
            client,
            options: RwLock::new(CallOptions::default()),
            endpoint: RwLock::new(None),
        }
    }
}

impl<T, S> OAuthClient<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    /// Return the public JWK set for this client's keyset, or an empty set if no keyset is configured.
    pub fn jwks(&self) -> JwkSet {
        self.registry
            .client_data
            .keyset
            .as_ref()
            .map(|keyset| keyset.public_jwks())
            .unwrap_or_default()
    }
    /// Begin an OAuth authorization flow and return the URL to which the user should be redirected.
    ///
    /// This resolves OAuth metadata for the given `input` (a handle, DID, or PDS/entryway URL),
    /// performs a Pushed Authorization Request (PAR) to the authorization server, persists the
    /// resulting state for later callback verification, and returns a fully-constructed
    /// authorization endpoint URL.
    ///
    /// The caller is responsible for redirecting the user's browser to the returned URL.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug", skip(self, input), fields(input = input.as_ref())))]
    pub async fn start_auth(
        &self,
        input: impl AsRef<str>,
        options: AuthorizeOptions<'_>,
    ) -> Result<String> {
        let client_metadata = atproto_client_metadata(
            self.registry.client_data.config.clone(),
            &self.registry.client_data.keyset,
        )?;
        let (server_metadata, identity) = self.client.resolve_oauth(input.as_ref()).await?;
        let login_hint = if identity.is_some() {
            Some(input.as_ref().into())
        } else {
            None
        };
        let metadata = OAuthMetadata {
            server_metadata,
            client_metadata,
            keyset: self.registry.client_data.keyset.clone(),
        };

        let auth_req_info = par(
            self.client.as_ref(),
            login_hint,
            options.prompt,
            &metadata,
            options.state,
        )
        .await?;

        // Persist state for callback handling
        self.registry
            .store
            .save_auth_req_info(&auth_req_info)
            .await?;

        #[derive(serde::Serialize)]
        struct Parameters<'s> {
            client_id: CowStr<'s>,
            request_uri: CowStr<'s>,
        }
        Ok(metadata.server_metadata.authorization_endpoint.to_string()
            + "?"
            + &serde_html_form::to_string(Parameters {
                client_id: metadata.client_metadata.client_id,
                request_uri: auth_req_info.request_uri,
            })
            .unwrap())
    }

    /// Complete the OAuth authorization flow after the authorization server redirects back to the client.
    ///
    /// Validates the `state` and optional `iss` parameters, exchanges the authorization code for
    /// tokens via the token endpoint, verifies the `sub` claim against the expected issuer, and
    /// persists the resulting session. On success returns an [`OAuthSession`] ready for API calls.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all, fields(state = params.state.as_ref().map(|s| s.as_ref()))))]
    pub async fn callback(&self, params: CallbackParams<'_>) -> Result<OAuthSession<T, S>> {
        let Some(state_key) = params.state else {
            return Err(CallbackError::MissingState.into());
        };

        let Some(auth_req_info) = self.registry.store.get_auth_req_info(&state_key).await? else {
            return Err(CallbackError::MissingState.into());
        };

        self.registry.store.delete_auth_req_info(&state_key).await?;

        let metadata = self
            .client
            .get_authorization_server_metadata(&auth_req_info.authserver_url.to_cowstr())
            .await?;

        if let Some(iss) = params.iss {
            if iss != metadata.issuer {
                return Err(CallbackError::IssuerMismatch {
                    expected: metadata.issuer.to_string(),
                    got: iss.to_string(),
                }
                .into());
            }
        } else if metadata.authorization_response_iss_parameter_supported == Some(true) {
            return Err(CallbackError::MissingIssuer.into());
        }
        let metadata = OAuthMetadata {
            server_metadata: metadata,
            client_metadata: atproto_client_metadata(
                self.registry.client_data.config.clone(),
                &self.registry.client_data.keyset,
            )?,
            keyset: self.registry.client_data.keyset.clone(),
        };
        let authserver_nonce = auth_req_info.dpop_data.dpop_authserver_nonce.clone();

        match exchange_code(
            self.client.as_ref(),
            &mut auth_req_info.dpop_data.clone(),
            &params.code,
            &auth_req_info.pkce_verifier,
            &metadata,
        )
        .await
        {
            Ok(token_set) => {
                let scopes = if let Some(scope) = &token_set.scope {
                    Scope::parse_multiple_reduced(&scope)
                        .expect("Failed to parse scopes")
                        .into_static()
                } else {
                    vec![]
                };
                let client_data = ClientSessionData {
                    account_did: token_set.sub.clone(),
                    session_id: auth_req_info.state,
                    host_url: Uri::parse(token_set.aud.as_ref())?.to_owned(),
                    authserver_url: auth_req_info.authserver_url.to_cowstr(),
                    authserver_token_endpoint: auth_req_info.authserver_token_endpoint,
                    authserver_revocation_endpoint: auth_req_info.authserver_revocation_endpoint,
                    scopes,
                    dpop_data: DpopClientData {
                        dpop_key: auth_req_info.dpop_data.dpop_key.clone(),
                        dpop_authserver_nonce: authserver_nonce.unwrap_or(CowStr::default()),
                        dpop_host_nonce: auth_req_info
                            .dpop_data
                            .dpop_authserver_nonce
                            .unwrap_or(CowStr::default()),
                    },
                    token_set,
                };

                self.create_session(client_data).await
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn create_session(&self, data: ClientSessionData<'_>) -> Result<OAuthSession<T, S>> {
        self.registry.set(data.clone()).await?;
        Ok(OAuthSession::new(
            self.registry.clone(),
            self.client.clone(),
            data.into_static(),
        ))
    }

    /// Restore a previously created session from the backing store, refreshing tokens if needed.
    pub async fn restore(&self, did: &Did<'_>, session_id: &str) -> Result<OAuthSession<T, S>> {
        self.create_session(self.registry.get(did, session_id, true).await?)
            .await
    }

    /// Revoke a session by deleting it from the backing store.
    ///
    /// Note: this removes the session from local storage but does **not** call the authorization
    /// server's revocation endpoint. To also invalidate the token server-side, prefer
    /// [`OAuthSession::logout`], which calls `revoke` on the token before deleting the session.
    pub async fn revoke(&self, did: &Did<'_>, session_id: &str) -> Result<()> {
        Ok(self.registry.del(did, session_id).await?)
    }
}

impl<T, S> HttpClient for OAuthClient<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    type Error = T::Error;

    async fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<Vec<u8>>, Self::Error> {
        self.client.send_http(request).await
    }
}

impl<T, S> IdentityResolver for OAuthClient<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    fn options(&self) -> &ResolverOptions {
        self.client.options()
    }

    async fn resolve_handle(
        &self,
        handle: &Handle<'_>,
    ) -> jacquard_identity::resolver::Result<Did<'static>> {
        self.client.resolve_handle(handle).await
    }

    async fn resolve_did_doc(
        &self,
        did: &Did<'_>,
    ) -> jacquard_identity::resolver::Result<DidDocResponse> {
        self.client.resolve_did_doc(did).await
    }
}

impl<T, S> XrpcClient for OAuthClient<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    async fn base_uri(&self) -> Uri<String> {
        self.endpoint.read().await.clone().unwrap_or_else(|| {
            Uri::parse("https://public.api.bsky.app")
                .expect("hardcoded URI is valid")
                .to_owned()
        })
    }

    async fn opts(&self) -> CallOptions<'_> {
        self.options.read().await.clone()
    }

    async fn set_opts(&self, opts: CallOptions<'_>) {
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
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let opts = self.options.read().await.clone();
        self.send_with_opts(request, opts).await
    }

    async fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions<'_>,
    ) -> XrpcResult<XrpcResponse<R>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let base_uri = self.base_uri().await;
        self.client
            .xrpc(base_uri)
            .with_options(opts.clone())
            .send(&request)
            .await
    }
}

/// An active OAuth session for a specific account, used to make authenticated API requests.
///
/// `OAuthSession` holds the DPoP-bound token set for one account and handles transparent
/// token refresh on `401 invalid_token` responses. The optional `W` type parameter allows
/// attaching a WebSocket client (defaults to `()` when WebSocket support is not needed).
///
/// Obtain an `OAuthSession` from [`OAuthClient::callback`] or [`OAuthClient::restore`].
pub struct OAuthSession<T, S, W = ()>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Shared registry used to persist and retrieve session data across refresh operations.
    pub registry: Arc<SessionRegistry<T, S>>,
    /// Underlying HTTP/identity/OAuth resolver shared with the parent `OAuthClient`.
    pub client: Arc<T>,
    /// Optional WebSocket client; `()` when WebSocket support is not required.
    pub ws_client: W,
    /// Mutable session data including DPoP key, nonces, and token set.
    pub data: RwLock<ClientSessionData<'static>>,
    /// Default call options applied to every outgoing XRPC request from this session.
    pub options: RwLock<CallOptions<'static>>,
}

impl<T, S> OAuthSession<T, S, ()>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Create a new session without a WebSocket client.
    ///
    /// This is the standard constructor used by [`OAuthClient::callback`] and
    /// [`OAuthClient::restore`]. For WebSocket support use [`OAuthSession::new_with_ws`].
    pub fn new(
        registry: Arc<SessionRegistry<T, S>>,
        client: Arc<T>,
        data: ClientSessionData<'static>,
    ) -> Self {
        Self {
            registry,
            client,
            ws_client: (),
            data: RwLock::new(data),
            options: RwLock::new(CallOptions::default()),
        }
    }
}

impl<T, S, W> OAuthSession<T, S, W>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Create a new session with an attached WebSocket client.
    ///
    /// Use this variant when the session needs to support WebSocket subscriptions in addition
    /// to standard XRPC calls. The `ws_client` is exposed via [`OAuthSession::ws_client`] and
    /// is used by the `WebSocketClient` impl when the `websocket` feature is enabled.
    pub fn new_with_ws(
        registry: Arc<SessionRegistry<T, S>>,
        client: Arc<T>,
        ws_client: W,
        data: ClientSessionData<'static>,
    ) -> Self {
        Self {
            registry,
            client,
            ws_client,
            data: RwLock::new(data),
            options: RwLock::new(CallOptions::default()),
        }
    }

    /// Consume this session and return a new one with the given call options pre-applied.
    ///
    /// Useful for setting request-level defaults (e.g., `atproto-proxy` or custom headers) once
    /// at construction time rather than passing them to every individual XRPC call.
    pub fn with_options(self, options: CallOptions<'_>) -> Self {
        Self {
            registry: self.registry,
            client: self.client,
            ws_client: self.ws_client,
            data: self.data,
            options: RwLock::new(options.into_static()),
        }
    }

    /// Get a reference to the WebSocket client.
    pub fn ws_client(&self) -> &W {
        &self.ws_client
    }

    /// Replace the default call options for this session without consuming it.
    pub async fn set_options(&self, options: CallOptions<'_>) {
        *self.options.write().await = options.into_static();
    }

    /// Return the DID and session ID for this session.
    ///
    /// The session ID is the random `state` token generated during the PAR flow and can
    /// be used together with the DID to restore the session via [`OAuthClient::restore`].
    pub async fn session_info(&self) -> (Did<'_>, CowStr<'_>) {
        let data = self.data.read().await;
        (data.account_did.clone(), data.session_id.clone())
    }

    /// Return the resource server (PDS) base URI for this session.
    pub async fn endpoint(&self) -> Uri<String> {
        self.data.read().await.host_url.clone()
    }

    /// Return the current DPoP-bound access token for this session.
    ///
    /// The token may be stale if it has expired; use [`OAuthSession::refresh`] or
    /// rely on the automatic refresh performed by `send_with_opts` to obtain a fresh one.
    pub async fn access_token(&self) -> AuthorizationToken<'_> {
        AuthorizationToken::Dpop(self.data.read().await.token_set.access_token.clone())
    }

    /// Return the current refresh token for this session, if one is present.
    ///
    /// Not all authorization servers issue refresh tokens. When `None` is returned,
    /// the session cannot be silently renewed and the user must re-authenticate.
    pub async fn refresh_token(&self) -> Option<AuthorizationToken<'_>> {
        self.data
            .read()
            .await
            .token_set
            .refresh_token
            .as_ref()
            .map(|t| AuthorizationToken::Dpop(t.clone()))
    }

    /// Derive an unauthenticated [`OAuthClient`] that shares the same registry and resolver.
    ///
    /// Useful when you need to initiate a new authorization flow from within an existing
    /// session context (e.g., to add a second account) without constructing a fresh client.
    pub fn to_client(&self) -> OAuthClient<T, S> {
        OAuthClient::from_session(self)
    }
}
impl<T, S, W> OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    /// Revoke the access token at the authorization server and delete the session from the store.
    ///
    /// Revocation is best-effort: if the server does not advertise a revocation endpoint, or if
    /// the revocation call fails, the session is still deleted locally. This prevents a dangling
    /// session record from blocking future logins for the same account.
    pub async fn logout(&self) -> Result<()> {
        use crate::request::{OAuthMetadata, revoke};
        let mut data = self.data.write().await;
        let meta =
            OAuthMetadata::new(self.client.as_ref(), &self.registry.client_data, &data).await?;
        if meta.server_metadata.revocation_endpoint.is_some() {
            let token = data.token_set.access_token.clone();
            revoke(self.client.as_ref(), &mut data.dpop_data, &token, &meta)
                .await
                .ok();
        }
        // Remove from store
        self.registry
            .del(&data.account_did, &data.session_id)
            .await?;
        Ok(())
    }
}

impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Construct an `OAuthClient` that shares the registry and resolver of an existing session.
    ///
    /// Equivalent to [`OAuthSession::to_client`]; provided on `OAuthClient` for symmetry so
    /// callers can obtain an unauthenticated client without holding a session reference.
    pub fn from_session<W>(session: &OAuthSession<T, S, W>) -> Self {
        Self {
            registry: session.registry.clone(),
            client: session.client.clone(),
            options: RwLock::new(CallOptions::default()),
            endpoint: RwLock::new(None),
        }
    }
}
impl<T, S, W> OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    /// Explicitly refresh the access token using the stored refresh token.
    ///
    /// On success the new token set is written back into both the in-memory session data and
    /// the backing store. The returned `AuthorizationToken` is the new access token, which
    /// callers can immediately use to retry a failed request.
    ///
    /// The actual token exchange is serialized per `(DID, session_id)` pair via a `Mutex` inside
    /// the registry, so concurrent refresh attempts will not result in duplicate token exchanges.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug", skip_all))]
    pub async fn refresh(&self) -> Result<AuthorizationToken<'_>> {
        // Read identifiers without holding the lock across await
        let (did, sid) = {
            let data = self.data.read().await;
            (data.account_did.clone(), data.session_id.clone())
        };
        let refreshed = self.registry.as_ref().get(&did, &sid, true).await?;
        let token = AuthorizationToken::Dpop(refreshed.token_set.access_token.clone());
        // Write back updated session
        *self.data.write().await = refreshed.clone().into_static();
        // Store in the registry
        self.registry.set(refreshed).await?;
        Ok(token)
    }
}

impl<T, S, W> HttpClient for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
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

impl<T, S, W> XrpcClient for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + XrpcExt + Send + Sync + 'static,
    W: Send + Sync,
{
    async fn base_uri(&self) -> Uri<String> {
        self.data.read().await.host_url.clone()
    }

    async fn opts(&self) -> CallOptions<'_> {
        self.options.read().await.clone()
    }

    async fn set_opts(&self, opts: CallOptions<'_>) {
        let mut guard = self.options.write().await;
        *guard = opts.into_static();
    }

    async fn set_base_uri(&self, uri: Uri<String>) {
        let normalized = jacquard_common::xrpc::normalize_base_uri(uri);
        let mut guard = self.data.write().await;
        guard.host_url = normalized;
    }

    async fn send<R>(&self, request: R) -> XrpcResult<XrpcResponse<R>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let opts = self.options.read().await.clone();
        self.send_with_opts(request, opts).await
    }

    async fn send_with_opts<R>(
        &self,
        request: R,
        mut opts: CallOptions<'_>,
    ) -> XrpcResult<XrpcResponse<R>>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let base_uri = self.base_uri().await;
        let original_token = self.access_token().await;
        opts.auth = Some(original_token.clone());
        // Clone dpop_data and release read lock before the await point
        let mut dpop = self.data.read().await.dpop_data.clone();
        let http_response = self
            .client
            .dpop_call(&mut dpop)
            .send(build_http_request(&base_uri, &request, &opts)?)
            .await
            .map_err(|e| ClientError::from(e).for_nsid(R::NSID))?;
        let resp = process_response(http_response);

        // Write back updated nonce to session data (dpop_call may have updated it)
        {
            let mut guard = self.data.write().await;
            guard.dpop_data.dpop_host_nonce = dpop.dpop_host_nonce.clone();
        }

        if is_invalid_token_response(&resp) {
            // Optimistic refresh: check if another request already refreshed the token
            let current_token = self.access_token().await;
            if current_token != original_token {
                // Token was already refreshed by another concurrent request, use it
                opts.auth = Some(current_token);
            } else {
                // We need to refresh - this will be serialized by the registry's Mutex
                opts.auth = Some(
                    self.refresh()
                        .await
                        .map_err(|e| ClientError::transport(e))?,
                );
            }
            // Re-read dpop_data after refresh (refresh may have updated it)
            let mut dpop = self.data.read().await.dpop_data.clone();
            let http_response = self
                .client
                .dpop_call(&mut dpop)
                .send(build_http_request(&base_uri, &request, &opts)?)
                .await
                .map_err(|e| {
                    ClientError::from(e)
                        .for_nsid(R::NSID)
                        .append_context("after token refresh")
                })?;
            let resp = process_response(http_response);

            // Write back updated nonce after retry
            {
                let mut guard = self.data.write().await;
                guard.dpop_data.dpop_host_nonce = dpop.dpop_host_nonce.clone();
            }

            resp
        } else {
            resp
        }
    }
}

#[cfg(feature = "streaming")]
impl<T, S, W> jacquard_common::http_client::HttpClientExt for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver
        + DpopExt
        + XrpcExt
        + jacquard_common::http_client::HttpClientExt
        + Send
        + Sync
        + 'static,
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
impl<T, S, W> jacquard_common::xrpc::XrpcStreamingClient for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver
        + DpopExt
        + XrpcExt
        + jacquard_common::http_client::HttpClientExt
        + Send
        + Sync
        + 'static,
    W: Send + Sync,
{
    async fn download<R>(
        &self,
        request: R,
    ) -> core::result::Result<jacquard_common::xrpc::StreamingResponse, jacquard_common::StreamError>
    where
        R: XrpcRequest + Send + Sync,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        use jacquard_common::StreamError;

        let base_uri = <Self as XrpcClient>::base_uri(self).await;
        let mut opts = self.options.read().await.clone();
        opts.auth = Some(self.access_token().await);
        let http_request = build_http_request(&base_uri, &request, &opts)
            .map_err(|e| StreamError::protocol(e.to_string()))?;
        let guard = self.data.read().await;
        let mut dpop = guard.dpop_data.clone();
        let result = self
            .client
            .dpop_call(&mut dpop)
            .send_streaming(http_request)
            .await;
        drop(guard);

        match result {
            Ok(response) => Ok(response),
            Err(_e) => {
                // Check if it's an auth error and retry
                opts.auth = Some(
                    self.refresh()
                        .await
                        .map_err(|e| StreamError::transport(e))?,
                );
                let http_request = build_http_request(&base_uri, &request, &opts)
                    .map_err(|e| StreamError::protocol(e.to_string()))?;
                let guard = self.data.read().await;
                let mut dpop = guard.dpop_data.clone();
                self.client
                    .dpop_call(&mut dpop)
                    .send_streaming(http_request)
                    .await
                    .map_err(StreamError::transport)
            }
        }
    }

    async fn stream<Str>(
        &self,
        stream: jacquard_common::xrpc::streaming::XrpcProcedureSend<Str::Frame<'static>>,
    ) -> core::result::Result<
        jacquard_common::xrpc::streaming::XrpcResponseStream<
            <<Str as jacquard_common::xrpc::streaming::XrpcProcedureStream>::Response as jacquard_common::xrpc::streaming::XrpcStreamResp>::Frame<'static>,
        >,
        jacquard_common::StreamError,
    >
    where
        Str: jacquard_common::xrpc::streaming::XrpcProcedureStream + 'static,
        <<Str as jacquard_common::xrpc::streaming::XrpcProcedureStream>::Response as jacquard_common::xrpc::streaming::XrpcStreamResp>::Frame<'static>: jacquard_common::xrpc::streaming::XrpcStreamResp,
    {
        use jacquard_common::StreamError;
        use n0_future::TryStreamExt;

        let base_uri = self.base_uri().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = Some(self.access_token().await);

        let mut path = String::from(base_uri.as_str().trim_end_matches('/'));
        path.push_str("/xrpc/");
        path.push_str(<Str::Request as jacquard_common::xrpc::XrpcRequest>::NSID);

        let mut builder = http::Request::post(path);

        if let Some(token) = &opts.auth {
            use jacquard_common::AuthorizationToken;
            let hv = match token {
                AuthorizationToken::Bearer(t) => {
                    http::HeaderValue::from_str(&format!("Bearer {}", t.as_ref()))
                }
                AuthorizationToken::Dpop(t) => {
                    http::HeaderValue::from_str(&format!("DPoP {}", t.as_ref()))
                }
            }
            .map_err(|e| StreamError::protocol(format!("Invalid authorization token: {}", e)))?;
            builder = builder.header(http::header::AUTHORIZATION, hv);
        }

        if let Some(proxy) = &opts.atproto_proxy {
            builder = builder.header("atproto-proxy", proxy.as_ref());
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

        let guard = self.data.read().await;
        let mut dpop = guard.dpop_data.clone();
        let result = self
            .client
            .dpop_call(&mut dpop)
            .send_bidirectional(parts, body_stream)
            .await;
        drop(guard);

        match result {
            Ok(response) => {
                let (resp_parts, resp_body) = response.into_parts();
                Ok(
                    jacquard_common::xrpc::streaming::XrpcResponseStream::from_typed_parts(
                        resp_parts, resp_body,
                    ),
                )
            }
            Err(e) => {
                // OAuth token refresh and retry is handled by dpop wrapper
                // If we get here, it's a real error
                Err(StreamError::transport(e))
            }
        }
    }
}

fn is_invalid_token_response<R: XrpcResp>(response: &XrpcResult<Response<R>>) -> bool {
    use jacquard_common::error::ClientErrorKind;

    match response {
        Err(e) => match e.kind() {
            ClientErrorKind::Auth(AuthError::InvalidToken) => true,
            ClientErrorKind::Auth(AuthError::Other(value)) => value
                .to_str()
                .is_ok_and(|s| s.starts_with("DPoP ") && s.contains("error=\"invalid_token\"")),
            _ => false,
        },
        Ok(resp) => match resp.parse() {
            Err(XrpcError::Auth(AuthError::InvalidToken)) => true,
            _ => false,
        },
    }
}

impl<T, S, W> IdentityResolver for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + IdentityResolver + XrpcExt + Send + Sync + 'static,
    W: Send + Sync,
{
    fn options(&self) -> &ResolverOptions {
        self.client.options()
    }

    fn resolve_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<Output = std::result::Result<Did<'static>, IdentityError>> {
        async { self.client.resolve_handle(handle).await }
    }

    fn resolve_did_doc(
        &self,
        did: &Did<'_>,
    ) -> impl Future<Output = std::result::Result<DidDocResponse, IdentityError>> {
        async { self.client.resolve_did_doc(did).await }
    }
}

#[cfg(feature = "websocket")]
impl<T, S, W> WebSocketClient for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + Send + Sync + 'static,
    W: WebSocketClient + Send + Sync,
{
    type Error = W::Error;

    async fn connect(
        &self,
        uri: Uri<&str>,
    ) -> std::result::Result<WebSocketConnection, Self::Error> {
        self.ws_client.connect(uri).await
    }

    async fn connect_with_headers(
        &self,
        uri: Uri<&str>,
        headers: Vec<(CowStr<'_>, CowStr<'_>)>,
    ) -> std::result::Result<WebSocketConnection, Self::Error> {
        self.ws_client.connect_with_headers(uri, headers).await
    }
}

#[cfg(feature = "websocket")]
impl<T, S, W> jacquard_common::xrpc::SubscriptionClient for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + Send + Sync + 'static,
    W: WebSocketClient + Send + Sync,
{
    async fn base_uri(&self) -> Uri<String> {
        self.data.read().await.host_url.clone()
    }

    async fn subscription_opts(&self) -> jacquard_common::xrpc::SubscriptionOptions<'_> {
        let mut opts = jacquard_common::xrpc::SubscriptionOptions::default();
        let token = self.access_token().await;
        let auth_value = match token {
            AuthorizationToken::Bearer(t) => format!("Bearer {}", t.as_ref()),
            AuthorizationToken::Dpop(t) => format!("DPoP {}", t.as_ref()),
        };
        opts.headers
            .push((CowStr::from("Authorization"), CowStr::from(auth_value)));
        opts
    }

    async fn subscribe<Sub>(
        &self,
        params: &Sub,
    ) -> std::result::Result<jacquard_common::xrpc::SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync,
    {
        let opts = self.subscription_opts().await;
        self.subscribe_with_opts(params, opts).await
    }

    async fn subscribe_with_opts<Sub>(
        &self,
        params: &Sub,
        opts: jacquard_common::xrpc::SubscriptionOptions<'_>,
    ) -> std::result::Result<jacquard_common::xrpc::SubscriptionStream<Sub::Stream>, Self::Error>
    where
        Sub: XrpcSubscription + Send + Sync,
    {
        use jacquard_common::xrpc::SubscriptionExt;
        let base = self.base_uri().await;
        self.subscription(base)
            .with_options(opts)
            .subscribe(params)
            .await
    }
}
