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
    error::{AuthError, ClientError, TransportError, XrpcResult},
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
use url::Url;

pub struct OAuthClient<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    pub registry: Arc<SessionRegistry<T, S>>,
    pub client: Arc<T>,
}

impl<S: ClientAuthStore> OAuthClient<JacquardResolver, S> {
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
    pub fn new_from_resolver(store: S, client: T, client_data: ClientData<'static>) -> Self {
        #[cfg(feature = "tracing")]
        tracing::info!(
            redirect_uris = ?client_data.config.redirect_uris,
            scopes = ?client_data.config.scopes,
            has_keyset = client_data.keyset.is_some(),
            "oauth client created"
        );

        let client = Arc::new(client);
        let registry = Arc::new(SessionRegistry::new(store, client.clone(), client_data));
        Self { registry, client }
    }

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
        Self { registry, client }
    }
}

impl<T, S> OAuthClient<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    pub fn jwks(&self) -> JwkSet {
        self.registry
            .client_data
            .keyset
            .as_ref()
            .map(|keyset| keyset.public_jwks())
            .unwrap_or_default()
    }
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
        let auth_req_info =
            par(self.client.as_ref(), login_hint, options.prompt, &metadata).await?;
        // Persist state for callback handling
        self.registry
            .store
            .save_auth_req_info(&auth_req_info)
            .await?;

        #[derive(serde::Serialize)]
        struct Parameters<'s> {
            client_id: Url,
            request_uri: CowStr<'s>,
        }
        Ok(metadata.server_metadata.authorization_endpoint.to_string()
            + "?"
            + &serde_html_form::to_string(Parameters {
                client_id: metadata.client_metadata.client_id.clone(),
                request_uri: auth_req_info.request_uri,
            })
            .unwrap())
    }

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
            .get_authorization_server_metadata(&auth_req_info.authserver_url)
            .await?;

        if let Some(iss) = params.iss {
            if !crate::resolver::issuer_equivalent(&iss, &metadata.issuer) {
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
                    host_url: Url::parse(&token_set.iss).expect("Failed to parse host URL"),
                    authserver_url: auth_req_info.authserver_url,
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

    pub async fn restore(&self, did: &Did<'_>, session_id: &str) -> Result<OAuthSession<T, S>> {
        self.create_session(self.registry.get(did, session_id, false).await?)
            .await
    }

    pub async fn revoke(&self, did: &Did<'_>, session_id: &str) -> Result<()> {
        Ok(self.registry.del(did, session_id).await?)
    }
}

pub struct OAuthSession<T, S, W = ()>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    pub registry: Arc<SessionRegistry<T, S>>,
    pub client: Arc<T>,
    pub ws_client: W,
    pub data: RwLock<ClientSessionData<'static>>,
    pub options: RwLock<CallOptions<'static>>,
}

impl<T, S> OAuthSession<T, S, ()>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
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

    pub async fn set_options(&self, options: CallOptions<'_>) {
        *self.options.write().await = options.into_static();
    }

    pub async fn session_info(&self) -> (Did<'_>, CowStr<'_>) {
        let data = self.data.read().await;
        (data.account_did.clone(), data.session_id.clone())
    }

    pub async fn endpoint(&self) -> Url {
        self.data.read().await.host_url.clone()
    }

    pub async fn access_token(&self) -> AuthorizationToken<'_> {
        AuthorizationToken::Dpop(self.data.read().await.token_set.access_token.clone())
    }

    pub async fn refresh_token(&self) -> Option<AuthorizationToken<'_>> {
        self.data
            .read()
            .await
            .token_set
            .refresh_token
            .as_ref()
            .map(|t| AuthorizationToken::Dpop(t.clone()))
    }
}
impl<T, S, W> OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
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
    pub fn from_session<W>(session: &OAuthSession<T, S, W>) -> Self {
        Self {
            registry: session.registry.clone(),
            client: session.client.clone(),
        }
    }
}
impl<T, S, W> OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
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
    async fn base_uri(&self) -> Url {
        self.data.read().await.host_url.clone()
    }

    async fn opts(&self) -> CallOptions<'_> {
        self.options.read().await.clone()
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
        opts.auth = Some(self.access_token().await);
        let guard = self.data.read().await;
        let mut dpop = guard.dpop_data.clone();
        let http_response = self
            .client
            .dpop_call(&mut dpop)
            .send(build_http_request(&base_uri, &request, &opts)?)
            .await
            .map_err(|e| TransportError::Other(Box::new(e)))?;
        let resp = process_response(http_response);
        drop(guard);
        if is_invalid_token_response(&resp) {
            opts.auth = Some(
                self.refresh()
                    .await
                    .map_err(|e| ClientError::Transport(TransportError::Other(e.into())))?,
            );
            let guard = self.data.read().await;
            let mut dpop = guard.dpop_data.clone();
            let http_response = self
                .client
                .dpop_call(&mut dpop)
                .send(build_http_request(&base_uri, &request, &opts)?)
                .await
                .map_err(|e| TransportError::Other(Box::new(e)))?;
            process_response(http_response)
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
        use n0_future::{StreamExt, TryStreamExt};

        let base_uri = self.base_uri().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = Some(self.access_token().await);

        let mut url = base_uri;
        let mut path = url.path().trim_end_matches('/').to_owned();
        path.push_str("/xrpc/");
        path.push_str(<Str::Request as jacquard_common::xrpc::XrpcRequest>::NSID);
        url.set_path(&path);

        let mut builder = http::Request::post(url.to_string());

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
            jacquard_common::stream::ByteStream::new(stream.0.map_ok(|f| f.buffer).boxed());

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
    match response {
        Err(ClientError::Auth(AuthError::InvalidToken)) => true,
        Err(ClientError::Auth(AuthError::Other(value))) => value
            .to_str()
            .is_ok_and(|s| s.starts_with("DPoP ") && s.contains("error=\"invalid_token\"")),
        Ok(resp) => match resp.parse() {
            Err(XrpcError::Auth(AuthError::InvalidToken)) => true,
            _ => false,
        },
        _ => false,
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

    async fn connect(&self, url: Url) -> std::result::Result<WebSocketConnection, Self::Error> {
        self.ws_client.connect(url).await
    }

    async fn connect_with_headers(
        &self,
        url: Url,
        headers: Vec<(CowStr<'_>, CowStr<'_>)>,
    ) -> std::result::Result<WebSocketConnection, Self::Error> {
        self.ws_client.connect_with_headers(url, headers).await
    }
}

#[cfg(feature = "websocket")]
impl<T, S, W> jacquard_common::xrpc::SubscriptionClient for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + Send + Sync + 'static,
    W: WebSocketClient + Send + Sync,
{
    async fn base_uri(&self) -> Url {
        #[cfg(not(target_arch = "wasm32"))]
        if tokio::runtime::Handle::try_current().is_ok() {
            return tokio::task::block_in_place(|| self.data.blocking_read().host_url.clone());
        }

        self.data.blocking_read().host_url.clone()
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
