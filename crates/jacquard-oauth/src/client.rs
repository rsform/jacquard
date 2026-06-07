use crate::{
    atproto::atproto_client_metadata,
    authstore::{ClientAuthStore, OAuthSessionMatch, OAuthSessionSelector},
    dpop::DpopExt,
    error::{CallbackError, OAuthError, Result},
    request::{OAuthMetadata, exchange_code, par},
    resolver::OAuthResolver,
    scopes::Scopes,
    session::{ClientData, ClientSessionData, DpopClientData, SessionRegistry},
    types::{AuthorizeOptions, CallbackParams},
};
#[cfg(feature = "scope-check")]
use crate::{
    error::ScopeError,
    resolver::resolve_permission_set,
    scopes::{IncludeScope, RepoCollection, RpcLexicon, Scope},
};
#[cfg(feature = "websocket")]
use jacquard_common::CowStr;
#[cfg(feature = "scope-check")]
use jacquard_common::types::nsid::Nsid;
use jacquard_common::{
    AuthorizationToken, IntoStatic,
    bos::BosStr,
    deps::fluent_uri::Uri,
    error::{AuthError, ClientError, XrpcResult},
    http_client::HttpClient,
    session::{SessionHint, SessionSelector, SessionStoreError},
    types::{did::Did, string::Handle},
    xrpc::{
        CallOptions, Response, XrpcClient, XrpcExt, XrpcRequest, XrpcResp, XrpcResponse,
        build_http_request, process_response,
    },
};
#[cfg(feature = "scope-check")]
use jacquard_identity::lexicon_resolver::LexiconSchemaResolver;

#[cfg(feature = "scope-check")]
use jacquard_common::deps::fluent_uri::pct_enc::{EStr, encoder::Query};

#[cfg(feature = "websocket")]
use jacquard_common::websocket::{WebSocketClient, WebSocketConnection};
#[cfg(feature = "websocket")]
use jacquard_common::xrpc::XrpcSubscription;
use jacquard_identity::{
    JacquardResolver,
    resolver::{DidDocResponse, IdentityError, IdentityResolver, ResolverOptions},
};
use jose_jwk::JwkSet;
use smol_str::{SmolStr, ToSmolStr};
use std::{str::FromStr, sync::Arc};
use tokio::sync::RwLock;

/// Result of resuming an OAuth session or starting a new authorization flow.
pub enum OAuthResumeOrLogin<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// A stored session was found and restored/refreshed.
    Resumed(OAuthSession<T, S>),
    /// No stored session matched; redirect the user to this login URL.
    LoginUrl(String),
    /// No stored session matched, and the hint did not contain enough information to start OAuth.
    NeedsInput,
}

/// The top-level OAuth client responsible for driving the authorization flow.
pub struct OAuthClient<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Shared session registry that mediates access to the backing auth store.
    pub registry: Arc<SessionRegistry<T, S, SmolStr>>,
    /// Default call options applied to every outgoing XRPC request.
    pub options: RwLock<CallOptions>,
    /// Override for the XRPC base URI; falls back to the public Bluesky AppView when `None`.
    pub endpoint: RwLock<Option<Uri<String>>>,
    /// Underlying HTTP/identity/OAuth resolver used for all network operations.
    pub client: Arc<T>,
}

impl<S: ClientAuthStore, C: HttpClient + Sync> OAuthClient<JacquardResolver<C>, S> {
    /// Create an `OAuthClient` using the default [`JacquardResolver`] for identity and metadata resolution.
    pub fn new(store: S, client_data: ClientData<SmolStr>, http: C) -> Self {
        let client = JacquardResolver::new(http, ResolverOptions::default());
        Self::new_from_resolver(store, client, client_data)
    }
}

impl<S: ClientAuthStore> OAuthClient<JacquardResolver<reqwest::Client>, S> {
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
        Self::new(store, client_data, reqwest::Client::new())
    }
}

impl OAuthClient<JacquardResolver<reqwest::Client>, crate::authstore::MemoryAuthStore> {
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
    pub fn new_from_resolver(store: S, client: T, client_data: ClientData<SmolStr>) -> Self {
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
        client_data: ClientData<SmolStr>,
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
    pub async fn start_auth<Str: BosStr>(
        &self,
        input: impl AsRef<str>,
        options: AuthorizeOptions<Str>,
    ) -> Result<String>
    where
        Str: FromStr + Ord + Clone + core::fmt::Debug,
        <Str as FromStr>::Err: core::fmt::Debug,
    {
        let client_metadata = atproto_client_metadata(
            &self.registry.client_data.config,
            &self.registry.client_data.keyset,
        )?;
        let (server_metadata, identity) = self.client.resolve_oauth(input.as_ref()).await?;
        let login_hint = if identity.is_some() {
            Some(input.as_ref().into())
        } else {
            None
        };
        let mut metadata = OAuthMetadata {
            server_metadata,
            client_metadata,
            keyset: self.registry.client_data.keyset.clone(),
        };

        let auth_req_info = par(
            self.client.as_ref(),
            login_hint,
            options.prompt,
            &mut metadata,
            options.state.map(|s| s.as_ref().to_smolstr()),
        )
        .await?;

        // Persist state for callback handling
        self.registry
            .store
            .save_auth_req_info(&auth_req_info)
            .await?;

        #[derive(serde::Serialize)]
        struct Parameters {
            client_id: smol_str::SmolStr,
            request_uri: smol_str::SmolStr,
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
    ///
    /// When the `scope-check` feature is enabled, this method also eagerly resolves any `include:`
    /// scopes by fetching the referenced permission sets. `T` must implement
    /// `LexiconSchemaResolver` in that case.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all, fields(state = params.state.as_ref().map(|s| s.as_str()))))]
    #[cfg(not(feature = "scope-check"))]
    pub async fn callback(&self, params: CallbackParams) -> Result<OAuthSession<T, S>> {
        let client_data = self.callback_core(params).await?;
        self.create_session(client_data).await
    }

    /// Complete the OAuth authorization flow (scope-check variant).
    ///
    /// Same as `callback`, but eagerly resolves `include:` scopes into
    /// concrete permissions via `LexiconSchemaResolver`.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all, fields(state = params.state.as_ref().map(|s| s.as_str()))))]
    #[cfg(feature = "scope-check")]
    pub async fn callback(&self, params: CallbackParams) -> Result<OAuthSession<T, S>>
    where
        T: LexiconSchemaResolver,
    {
        let mut client_data = self.callback_core(params).await?;
        client_data.resolved_scopes =
            Some(resolve_include_scopes(self.client.as_ref(), &client_data.scopes).await?);
        self.create_session(client_data).await
    }

    /// Shared callback logic: validate state/iss, exchange code, build session data.
    async fn callback_core(&self, params: CallbackParams) -> Result<ClientSessionData> {
        let Some(state_key) = params.state else {
            return Err(CallbackError::MissingState.into());
        };

        let Some(auth_req_info) = self
            .registry
            .store
            .get_auth_req_info(state_key.as_str())
            .await?
        else {
            return Err(CallbackError::MissingState.into());
        };

        self.registry
            .store
            .delete_auth_req_info(state_key.as_str())
            .await?;

        let metadata = self
            .client
            .get_authorization_server_metadata(auth_req_info.authserver_url.as_str())
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
                &self.registry.client_data.config,
                &self.registry.client_data.keyset,
            )?,
            keyset: self.registry.client_data.keyset.clone(),
        };
        let authserver_nonce = auth_req_info.dpop_data.dpop_authserver_nonce.clone();

        match exchange_code(
            self.client.as_ref(),
            &mut auth_req_info.dpop_data.clone(),
            params.code.as_str(),
            auth_req_info.pkce_verifier.as_str(),
            &metadata,
        )
        .await
        {
            Ok(token_set) => {
                let scopes = if let Some(scope) = &token_set.scope {
                    Scopes::new(scope.as_str().to_smolstr())
                        .expect("Failed to parse scopes from token response")
                } else {
                    Scopes::empty()
                };
                Ok(ClientSessionData {
                    account_did: token_set.sub.clone(),
                    session_id: auth_req_info.state,
                    host_url: Uri::parse(token_set.aud.as_str())?.to_owned(),
                    authserver_url: auth_req_info.authserver_url,
                    authserver_token_endpoint: auth_req_info.authserver_token_endpoint,
                    authserver_revocation_endpoint: auth_req_info.authserver_revocation_endpoint,
                    scopes,
                    dpop_data: DpopClientData {
                        dpop_key: auth_req_info.dpop_data.dpop_key.clone(),
                        dpop_authserver_nonce: authserver_nonce.unwrap_or_default(),
                        dpop_host_nonce: auth_req_info
                            .dpop_data
                            .dpop_authserver_nonce
                            .unwrap_or_default(),
                    },
                    token_set,
                    resolved_scopes: None,
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn create_session(&self, data: ClientSessionData) -> Result<OAuthSession<T, S>> {
        self.registry.set(data.clone()).await?;
        Ok(OAuthSession::new(
            self.registry.clone(),
            self.client.clone(),
            data.into_static(),
        ))
    }

    /// Restore a previously created session from the backing store, refreshing tokens if needed.
    pub async fn restore(
        &self,
        did: &Did<impl BosStr + Send + Sync>,
        session_id: &str,
    ) -> Result<OAuthSession<T, S>> {
        self.create_session(self.registry.get(did, session_id, true).await?)
            .await
    }

    /// Resume a stored session for `input`, or begin OAuth authorization and return a login URL.
    pub async fn resume_or_start_auth_for<Str: BosStr>(
        &self,
        input: impl AsRef<str>,
        options: AuthorizeOptions<Str>,
    ) -> Result<OAuthResumeOrLogin<T, S>>
    where
        S: SessionSelector<OAuthSessionMatch, Error = SessionStoreError>,
        Str: FromStr + Ord + Clone + core::fmt::Debug,
        <Str as FromStr>::Err: core::fmt::Debug,
    {
        let input = input.as_ref();
        self.resume_or_start_auth(&SessionHint::from_input(input), options)
            .await
    }

    /// Resume a stored session for `hint`, or begin OAuth authorization from the hint identity.
    pub async fn resume_or_start_auth<HintStr, Str>(
        &self,
        hint: &SessionHint<HintStr>,
        options: AuthorizeOptions<Str>,
    ) -> Result<OAuthResumeOrLogin<T, S>>
    where
        S: SessionSelector<OAuthSessionMatch, Error = SessionStoreError>,
        HintStr: BosStr + Send + Sync,
        Str: BosStr + FromStr + Ord + Clone + core::fmt::Debug,
        <Str as FromStr>::Err: core::fmt::Debug,
    {
        let input = oauth_start_auth_input_from_hint(hint);
        match OAuthSessionSelector::new(self.registry.store.as_ref(), self.client.as_ref())
            .select_session(hint)
            .await?
        {
            Some(matched) => match self
                .restore(&matched.key.did, matched.key.session_id.as_str())
                .await
            {
                Ok(session) => Ok(OAuthResumeOrLogin::Resumed(session)),
                Err(err) if should_start_auth_after_restore_error(&err) => {
                    let Some(input) = input else {
                        return Err(err);
                    };
                    Ok(OAuthResumeOrLogin::LoginUrl(
                        self.start_auth(input, options).await?,
                    ))
                }
                Err(err) => Err(err),
            },
            None => {
                let Some(input) = input else {
                    return Ok(OAuthResumeOrLogin::NeedsInput);
                };
                Ok(OAuthResumeOrLogin::LoginUrl(
                    self.start_auth(input, options).await?,
                ))
            }
        }
    }

    /// Revoke a session by deleting it from the backing store.
    ///
    /// Note: this removes the session from local storage but does **not** call the authorization
    /// server's revocation endpoint. To also invalidate the token server-side, prefer
    /// [`OAuthSession::logout`], which calls `revoke` on the token before deleting the session.
    pub async fn revoke(
        &self,
        did: &Did<impl BosStr + Send + Sync>,
        session_id: &str,
    ) -> Result<()> {
        Ok(self.registry.del(did, session_id).await?)
    }
}

fn oauth_start_auth_input_from_hint<S: BosStr>(hint: &SessionHint<S>) -> Option<SmolStr> {
    match hint {
        SessionHint::Did(did) => Some(did.as_ref().to_smolstr()),
        SessionHint::Handle(handle) => Some(handle.as_ref().to_smolstr()),
        SessionHint::Key(key) => Some(key.did.as_str().to_smolstr()),
        SessionHint::Identifier(identifier) => Some(identifier.as_ref().to_smolstr()),
        SessionHint::Any => None,
    }
}

fn should_start_auth_after_restore_error(err: &OAuthError) -> bool {
    matches!(err, OAuthError::Session(session_err) if session_err.is_permanent())
}

/// Decode a percent-encoded audience string.
///
/// The audience may contain percent-encoded characters like `%23` for `#`.
/// This function decodes those and returns the decoded string.
#[cfg(feature = "scope-check")]
fn decode_audience(aud: &str) -> Result<String> {
    // Use fluent_uri's percent-decoding to handle encoded characters.
    // The audience is typically a DID, possibly with a fragment.
    // EStr::new returns Option<&EStr>, so we match on that.
    match EStr::<Query>::new(aud) {
        Some(estr) => {
            // estr.decode() returns a Decode struct
            // The Decode type has a to_string() method that returns Result<Cow<str>, Vec<u8>>
            let decoded = estr.decode();
            match decoded.to_string() {
                Ok(cow) => Ok(cow.into_owned()),
                Err(bytes) => Err(crate::error::CallbackError::ScopeResolution {
                    detail: format!(
                        "percent-decoded audience contains invalid UTF-8: {:?}",
                        bytes
                    ),
                }
                .into()),
            }
        }
        None => {
            // If it's not a valid percent-encoded string, use it as-is.
            // This handles cases where no encoding was applied.
            Ok(aud.to_string())
        }
    }
}

/// Resolve all `include:` scopes in the given scope set into concrete permissions.
///
/// Non-include scopes are passed through unchanged. Each `include:` scope is
/// resolved via `resolve_permission_set`, which fetches the permission set
/// lexicon and expands it into concrete `Scope<SmolStr>` values.
#[cfg(feature = "scope-check")]
async fn resolve_include_scopes<R>(
    resolver: &R,
    scopes: &Scopes<SmolStr>,
) -> Result<Vec<Scope<SmolStr>>>
where
    R: OAuthResolver + LexiconSchemaResolver + Send + Sync,
{
    let mut resolved = Vec::new();
    for scope in scopes.iter() {
        match scope {
            Scope::Include(IncludeScope { nsid, audience }) => {
                let audience_did = if let Some(aud_str) = audience {
                    let decoded = decode_audience(aud_str)?;
                    match Did::new_owned(&decoded) {
                        Ok(did) => Some(did),
                        Err(_) => {
                            return Err(crate::error::CallbackError::ScopeResolution {
                                detail: format!(
                                    "invalid DID in include scope audience: {}",
                                    decoded
                                ),
                            }
                            .into());
                        }
                    }
                } else {
                    None
                };

                let nsid_smolstr = match Nsid::<SmolStr>::new_owned(nsid.as_str()) {
                    Ok(n) => n,
                    Err(_) => {
                        return Err(crate::error::CallbackError::ScopeResolution {
                            detail: format!("invalid NSID in include scope: {}", nsid),
                        }
                        .into());
                    }
                };

                let expanded =
                    resolve_permission_set(resolver, &nsid_smolstr, audience_did.as_ref()).await?;
                resolved.extend(expanded);
            }
            other => {
                resolved.push(other.convert());
            }
        }
    }
    Ok(resolved)
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

    async fn resolve_handle<Str: BosStr + Sync>(
        &self,
        handle: &Handle<Str>,
    ) -> jacquard_identity::resolver::Result<Did> {
        self.client.resolve_handle(handle).await
    }

    async fn resolve_did_doc<Str: BosStr + Sync>(
        &self,
        did: &Did<Str>,
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

    async fn send_with_opts<R>(&self, request: R, opts: CallOptions) -> XrpcResult<XrpcResponse<R>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        let base_uri = self.base_uri().await;
        let http_request = build_http_request(&base_uri.borrow(), &request, &opts)?;
        let http_response = self
            .client
            .send_http(http_request)
            .await
            .map_err(|e| ClientError::transport(e).for_nsid(R::NSID))?;
        process_response(http_response)
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
    pub registry: Arc<SessionRegistry<T, S, SmolStr>>,
    /// Underlying HTTP/identity/OAuth resolver shared with the parent `OAuthClient`.
    pub client: Arc<T>,
    /// Optional WebSocket client; `()` when WebSocket support is not required.
    pub ws_client: W,
    /// Mutable session data including DPoP key, nonces, and token set.
    pub data: RwLock<ClientSessionData>,
    /// Default call options applied to every outgoing XRPC request from this session.
    pub options: RwLock<CallOptions>,
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
        registry: Arc<SessionRegistry<T, S, SmolStr>>,
        client: Arc<T>,
        data: ClientSessionData,
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
        registry: Arc<SessionRegistry<T, S, SmolStr>>,
        client: Arc<T>,
        ws_client: W,
        data: ClientSessionData,
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
    pub fn with_options(self, options: CallOptions) -> Self {
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
    pub async fn set_options(&self, options: CallOptions) {
        *self.options.write().await = options.into_static();
    }

    /// Return the DID and session ID for this session.
    ///
    /// The session ID is the random `state` token generated during the PAR flow and can
    /// be used together with the DID to restore the session via [`OAuthClient::restore`].
    pub async fn session_info(&self) -> (Did, smol_str::SmolStr) {
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
    pub async fn access_token(&self) -> AuthorizationToken<SmolStr> {
        AuthorizationToken::Dpop(self.data.read().await.token_set.access_token.clone())
    }

    /// Return the current refresh token for this session, if one is present.
    ///
    /// Not all authorization servers issue refresh tokens. When `None` is returned,
    /// the session cannot be silently renewed and the user must re-authenticate.
    pub async fn refresh_token(&self) -> Option<AuthorizationToken<SmolStr>> {
        self.data
            .read()
            .await
            .token_set
            .refresh_token
            .clone()
            .map(|t| AuthorizationToken::Dpop(t))
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
    pub async fn refresh(&self) -> Result<AuthorizationToken<SmolStr>> {
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

    async fn opts(&self) -> CallOptions {
        self.options.read().await.clone()
    }

    async fn set_opts(&self, opts: CallOptions) {
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
        // Pre-flight scope check: pure in-memory, no HTTP.
        #[cfg(feature = "scope-check")]
        {
            self.check_scope::<R>().await.map_err(|e| {
                ClientError::invalid_request(format!("scope check failed: {:?}", e))
                    .for_nsid(R::NSID)
            })?;
        }

        let base_uri = self.base_uri().await;
        let original_token = self.access_token().await;
        opts.auth = Some(original_token.clone());
        // Clone dpop_data and release read lock before the await point
        let mut dpop = self.data.read().await.dpop_data.clone();
        let http_response = self
            .client
            .dpop_call(&mut dpop)
            .send(build_http_request(&base_uri.borrow(), &request, &opts)?)
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
                .send(build_http_request(&base_uri.borrow(), &request, &opts)?)
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

#[cfg(feature = "scope-check")]
impl<T, S, W> OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + Send + Sync + 'static,
    W: Send + Sync,
{
    /// Check whether the session's resolved scopes grant access to
    /// the XRPC method identified by `R::NSID`.
    async fn check_scope<R: XrpcRequest>(&self) -> core::result::Result<(), ScopeError> {
        let data = self.data.read().await;

        // Use the resolved scopes from Phase 5's eager resolution.
        // These are fully expanded — no include scopes remain.
        let resolved = data.resolved_scopes.as_ref();

        let is_permitted = match resolved {
            Some(scopes) => {
                let nsid = Nsid::<SmolStr>::new_static(R::NSID).expect("valid NSID");

                // Check if any granted scope covers this NSID. A request
                // may be covered by rpc: scopes (method access) or repo:
                // scopes (record operations).
                //
                // Note: `atproto` is the minimum base scope (auth only).
                // It does NOT grant rpc/repo access.
                //
                // For rpc: scopes, we check only the lxm (method) match
                // and ignore audience. At pre-flight time the client does
                // not know the target audience — audience enforcement is
                // the server's responsibility. A granted scope with a
                // specific aud (e.g., did:web:api.bsky.app) still permits
                // calling the method from the client's perspective.
                let rpc_ok = scopes.iter().any(|s| match s {
                    Scope::Rpc(rpc) => rpc.lxm.iter().any(|l| match l {
                        RpcLexicon::All => true,
                        RpcLexicon::Nsid(granted_nsid) => granted_nsid.as_ref() == nsid.as_ref(),
                    }),
                    _ => false,
                });

                // For repo: scopes, check if the NSID matches a granted
                // collection. Any action suffices for pre-flight.
                let repo_ok = scopes.iter().any(|s| match s {
                    Scope::Repo(repo) => match &repo.collection {
                        RepoCollection::All => true,
                        RepoCollection::Nsid(col) => col.as_ref() == nsid.as_ref(),
                    },
                    _ => false,
                });

                rpc_ok || repo_ok
            }
            None => {
                // No resolved scopes means resolution was skipped
                // (e.g., no include scopes were present, or scope-check
                // was enabled after session creation). Allow the request.
                true
            }
        };

        if !is_permitted {
            let granted_summary = resolved
                .map(|scopes| {
                    scopes
                        .iter()
                        .map(|s| s.to_string_normalized())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            return Err(ScopeError {
                nsid: SmolStr::new_static(R::NSID),
                granted: SmolStr::from(granted_summary),
            });
        }

        Ok(())
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
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        use jacquard_common::StreamError;

        let base_uri = <Self as XrpcClient>::base_uri(self).await;
        let mut opts = self.options.read().await.clone();
        opts.auth = Some(self.access_token().await);
        let http_request = build_http_request(&base_uri.borrow(), &request, &opts)
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
                let http_request = build_http_request(&base_uri.borrow(), &request, &opts)
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
        Str: jacquard_common::xrpc::streaming::XrpcProcedureStream + 'static,
        <<Str as jacquard_common::xrpc::streaming::XrpcProcedureStream>::Response as jacquard_common::xrpc::streaming::XrpcStreamResp>::Frame<B>: jacquard_common::xrpc::streaming::XrpcStreamResp,
        B: BosStr + 'static,
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
                    http::HeaderValue::from_str(&format!("Bearer {}", t.as_str()))
                }
                AuthorizationToken::Dpop(t) => {
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
                    jacquard_common::xrpc::streaming::XrpcResponseStream::from_typed_parts::<B>(
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
        // Some servers return 200/401 with an error in the body rather than using
        // WWW-Authenticate. Check the raw response bytes for the invalid_token pattern.
        Ok(resp) => {
            resp.status() == http::StatusCode::UNAUTHORIZED
                || serde_json::from_slice::<serde_json::Value>(resp.buffer())
                    .ok()
                    .and_then(|v| v.get("error")?.as_str().map(|s| s == "invalid_token"))
                    .unwrap_or(false)
        }
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

    async fn resolve_handle<Str: BosStr + Sync>(
        &self,
        handle: &Handle<Str>,
    ) -> std::result::Result<Did, IdentityError> {
        self.client.resolve_handle(handle).await
    }

    async fn resolve_did_doc<Str: BosStr + Sync>(
        &self,
        did: &Did<Str>,
    ) -> std::result::Result<DidDocResponse, IdentityError> {
        self.client.resolve_did_doc(did).await
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
            AuthorizationToken::Bearer(t) => format!("Bearer {}", t.as_str()),
            AuthorizationToken::Dpop(t) => format!("DPoP {}", t.as_str()),
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
        Sub: XrpcSubscription + Send + Sync + serde::Serialize,
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

#[cfg(all(test, feature = "scope-check"))]
mod tests {
    use super::*;
    use crate::scopes::{RepoAction, RepoScope, RpcAudience, RpcLexicon, RpcScope};
    use std::collections::BTreeSet;

    /// Test that a scope granting access to an RPC method works correctly.
    #[test]
    fn test_scope_check_permits_matching_rpc() {
        // AC7.1: Session with rpc:com.example.test grants access to com.example.test.
        let mut rpc_scope_set = BTreeSet::new();
        rpc_scope_set.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
        ));
        let mut aud_set = BTreeSet::new();
        aud_set.insert(RpcAudience::All);

        let granted_scope = Scope::Rpc(RpcScope {
            lxm: rpc_scope_set,
            aud: aud_set,
        });

        // Target scope for a request to com.example.test.
        let mut target_lxm = BTreeSet::new();
        target_lxm.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
        ));
        let mut target_aud = BTreeSet::new();
        target_aud.insert(RpcAudience::All);

        let target_scope = Scope::Rpc(RpcScope {
            lxm: target_lxm,
            aud: target_aud,
        });

        // The granted scope should permit the target scope.
        assert!(
            granted_scope.grants(&target_scope),
            "rpc:com.example.test should grant access to com.example.test"
        );
    }

    /// Test that rpc:* wildcard grants access to all RPC methods.
    #[test]
    fn test_scope_check_permits_rpc_wildcard() {
        // AC7.1: Session with rpc:* (wildcard) grants access to any RPC method.
        let mut rpc_scope_set: BTreeSet<RpcLexicon<SmolStr>> = BTreeSet::new();
        rpc_scope_set.insert(RpcLexicon::All);
        let mut aud_set: BTreeSet<RpcAudience<SmolStr>> = BTreeSet::new();
        aud_set.insert(RpcAudience::All);

        let wildcard_scope = Scope::Rpc(RpcScope {
            lxm: rpc_scope_set,
            aud: aud_set,
        });

        // Target scope for any request.
        let mut target_lxm = BTreeSet::new();
        target_lxm.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
        ));
        let mut target_aud = BTreeSet::new();
        target_aud.insert(RpcAudience::All);

        let target_scope = Scope::Rpc(RpcScope {
            lxm: target_lxm,
            aud: target_aud,
        });

        // Wildcard should grant any target scope.
        assert!(
            wildcard_scope.grants(&target_scope),
            "rpc:* should grant access to any RPC method"
        );
    }

    /// Test that an unmatched scope denies access.
    #[test]
    fn test_scope_check_denies_ungranted() {
        // AC7.4: Session with rpc:com.example.other denies access to com.example.test.
        let mut rpc_scope_set = BTreeSet::new();
        rpc_scope_set.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.other").unwrap(),
        ));
        let mut aud_set = BTreeSet::new();
        aud_set.insert(RpcAudience::All);

        let granted_scope = Scope::Rpc(RpcScope {
            lxm: rpc_scope_set,
            aud: aud_set,
        });

        // Target scope for a request to com.example.test.
        let mut target_lxm = BTreeSet::new();
        target_lxm.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
        ));
        let mut target_aud = BTreeSet::new();
        target_aud.insert(RpcAudience::All);

        let target_scope = Scope::Rpc(RpcScope {
            lxm: target_lxm,
            aud: target_aud,
        });

        // rpc:com.example.other should NOT grant access to com.example.test.
        assert!(
            !granted_scope.grants(&target_scope),
            "rpc:com.example.other should NOT grant access to com.example.test"
        );
    }

    /// Test that a repo scope grants access to the specified collection.
    #[test]
    fn test_scope_check_permits_repo_scope() {
        // AC7.1: Session with repo:com.example.test grants access to that collection.
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        actions.insert(RepoAction::Update);
        actions.insert(RepoAction::Delete);

        let granted_repo = Scope::Repo(RepoScope {
            collection: RepoCollection::Nsid(
                Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
            ),
            actions,
        });

        // Target scope for a request to com.example.test.
        let mut target_actions = BTreeSet::new();
        target_actions.insert(RepoAction::Create);
        target_actions.insert(RepoAction::Update);
        target_actions.insert(RepoAction::Delete);

        let target_repo = Scope::Repo(RepoScope {
            collection: RepoCollection::Nsid(
                Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
            ),
            actions: target_actions,
        });

        // The repo scope should grant the target scope.
        assert!(
            granted_repo.grants(&target_repo),
            "repo:com.example.test should grant repo access to com.example.test"
        );
    }

    /// Test that ScopeError provides diagnostic information.
    #[test]
    fn test_scope_error_diagnostic_info() {
        // AC7.4: ScopeError includes request NSID and granted scope summary.
        let err = ScopeError {
            nsid: SmolStr::from("com.example.test"),
            granted: SmolStr::from("rpc:com.example.other"),
        };

        assert_eq!(err.nsid, "com.example.test");
        assert_eq!(err.granted, "rpc:com.example.other");
        let error_msg = err.to_string();
        assert!(
            error_msg.contains("not permitted"),
            "error message should indicate request is not permitted"
        );
    }

    /// Test that multiple granted scopes are checked correctly.
    #[test]
    fn test_scope_check_multiple_scopes() {
        // AC7.1: With multiple scopes, request matching one of them is permitted.
        let mut other_lxm = BTreeSet::new();
        other_lxm.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.other").unwrap(),
        ));
        let mut other_aud = BTreeSet::new();
        other_aud.insert(RpcAudience::All);

        let other_scope = Scope::Rpc(RpcScope {
            lxm: other_lxm,
            aud: other_aud,
        });

        let mut test_lxm = BTreeSet::new();
        test_lxm.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
        ));
        let mut test_aud = BTreeSet::new();
        test_aud.insert(RpcAudience::All);

        let test_scope = Scope::Rpc(RpcScope {
            lxm: test_lxm,
            aud: test_aud,
        });

        // Target scope for a request to com.example.test.
        let mut target_lxm = BTreeSet::new();
        target_lxm.insert(RpcLexicon::Nsid(
            Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
        ));
        let mut target_aud = BTreeSet::new();
        target_aud.insert(RpcAudience::All);

        let target_scope = Scope::Rpc(RpcScope {
            lxm: target_lxm,
            aud: target_aud,
        });

        // With multiple scopes, if one matches, the check passes.
        let granted_scopes = vec![other_scope, test_scope];
        let is_permitted = granted_scopes.iter().any(|s| s.grants(&target_scope));
        assert!(
            is_permitted,
            "at least one granted scope should permit the target request"
        );
    }

    /// Test that both RPC and repo scopes are checked when determining permissions.
    #[test]
    fn test_scope_check_rpc_and_repo_paths() {
        // AC7.1: A request can be granted via either rpc: or repo: scopes.

        // Create a repo scope for the collection.
        let mut repo_actions = BTreeSet::new();
        repo_actions.insert(RepoAction::Create);
        repo_actions.insert(RepoAction::Update);
        repo_actions.insert(RepoAction::Delete);

        let repo_scope = Scope::Repo(RepoScope {
            collection: RepoCollection::Nsid(
                Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
            ),
            actions: repo_actions,
        });

        // Target scope for a request to com.example.test (as repo operations).
        let mut target_actions = BTreeSet::new();
        target_actions.insert(RepoAction::Create);
        target_actions.insert(RepoAction::Update);
        target_actions.insert(RepoAction::Delete);

        let target_repo = Scope::Repo(RepoScope {
            collection: RepoCollection::Nsid(
                Nsid::<SmolStr>::new_static("com.example.test").unwrap(),
            ),
            actions: target_actions,
        });

        // The repo scope should satisfy the request.
        assert!(
            repo_scope.grants(&target_repo),
            "repo scope should grant repo-based requests"
        );
    }
}
