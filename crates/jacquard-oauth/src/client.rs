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
        CallOptions, Response, XrpcClient, XrpcExt, XrpcRequest, XrpcResp, build_http_request,
        process_response,
    },
};
use jacquard_identity::{JacquardResolver, resolver::IdentityResolver};
use jose_jwk::JwkSet;
use std::sync::Arc;
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

pub struct OAuthSession<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    pub registry: Arc<SessionRegistry<T, S>>,
    pub client: Arc<T>,
    pub data: RwLock<ClientSessionData<'static>>,
    pub options: RwLock<CallOptions<'static>>,
}

impl<T, S> OAuthSession<T, S>
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
            data: RwLock::new(data),
            options: RwLock::new(CallOptions::default()),
        }
    }

    pub fn with_options(self, options: CallOptions<'_>) -> Self {
        Self {
            registry: self.registry,
            client: self.client,
            data: self.data,
            options: RwLock::new(options.into_static()),
        }
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
impl<T, S> OAuthSession<T, S>
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
    pub fn from_session(session: &OAuthSession<T, S>) -> Self {
        Self {
            registry: session.registry.clone(),
            client: session.client.clone(),
        }
    }
}
impl<T, S> OAuthSession<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
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

impl<T, S> HttpClient for OAuthSession<T, S>
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

impl<T, S> XrpcClient for OAuthSession<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + XrpcExt + Send + Sync + 'static,
{
    fn base_uri(&self) -> Url {
        // base_uri is a synchronous trait method; we must avoid async `.read().await`.
        // Use `block_in_place` under Tokio to perform a blocking RwLock read safely.
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| self.data.blocking_read().host_url.clone())
        } else {
            self.data.blocking_read().host_url.clone()
        }
    }

    async fn opts(&self) -> CallOptions<'_> {
        self.options.read().await.clone()
    }

    async fn send<'s, R>(
        &self,
        request: R,
    ) -> XrpcResult<Response<<R as XrpcRequest<'s>>::Response>>
    where
        R: XrpcRequest<'s>,
    {
        let base_uri = self.base_uri();
        let mut opts = self.options.read().await.clone();
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

fn is_invalid_token_response<R: XrpcResp>(response: &XrpcResult<Response<R>>) -> bool {
    match response {
        Err(ClientError::Auth(AuthError::InvalidToken)) => true,
        Err(ClientError::Auth(AuthError::Other(value))) => value
            .to_str()
            .is_ok_and(|s| s.starts_with("DPoP ") && s.contains("error=\"invalid_token\"")),
        Ok(resp) => match resp.parse() {
            Err(jacquard_common::xrpc::XrpcError::Auth(AuthError::InvalidToken)) => true,
            _ => false,
        },
        _ => false,
    }
}

impl<T, S> IdentityResolver for OAuthSession<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + IdentityResolver + XrpcExt + Send + Sync + 'static,
{
    fn options(&self) -> &jacquard_identity::resolver::ResolverOptions {
        self.client.options()
    }

    fn resolve_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<
        Output = std::result::Result<Did<'static>, jacquard_identity::resolver::IdentityError>,
    > {
        async { self.client.resolve_handle(handle).await }
    }

    fn resolve_did_doc(
        &self,
        did: &Did<'_>,
    ) -> impl Future<
        Output = std::result::Result<
            jacquard_identity::resolver::DidDocResponse,
            jacquard_identity::resolver::IdentityError,
        >,
    > {
        async { self.client.resolve_did_doc(did).await }
    }
}
