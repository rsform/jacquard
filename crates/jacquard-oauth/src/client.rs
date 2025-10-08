use crate::{
    atproto::atproto_client_metadata,
    authstore::ClientAuthStore,
    dpop::DpopExt,
    error::{OAuthError, Result},
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
    types::{
        did::Did,
        xrpc::{CallOptions, Response, XrpcClient, XrpcExt, XrpcRequest},
    },
};
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
            return Err(OAuthError::Callback("missing state parameter".into()));
        };

        let Some(auth_req_info) = self.registry.store.get_auth_req_info(&state_key).await? else {
            return Err(OAuthError::Callback(format!(
                "unknown authorization state: {state_key}"
            )));
        };

        self.registry.store.delete_auth_req_info(&state_key).await?;

        let metadata = self
            .client
            .get_authorization_server_metadata(&auth_req_info.authserver_url)
            .await?;

        if let Some(iss) = params.iss {
            if iss != metadata.issuer {
                return Err(OAuthError::Callback(format!(
                    "issuer mismatch: expected {}, got {iss}",
                    metadata.issuer
                )));
            }
        } else if metadata.authorization_response_iss_parameter_supported == Some(true) {
            return Err(OAuthError::Callback("missing `iss` parameter".into()));
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
            .map(|token| AuthorizationToken::Dpop(token.clone()))
    }
}
impl<T, S> OAuthSession<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    pub async fn refresh(&self) -> Result<AuthorizationToken<'_>> {
        let mut data = self.data.write().await;
        let refreshed = self
            .registry
            .as_ref()
            .get(&data.account_did, &data.session_id, true)
            .await?;
        let token = AuthorizationToken::Dpop(refreshed.token_set.access_token.clone());
        *data = refreshed.into_static();
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
        self.data.blocking_read().host_url.clone()
    }

    async fn opts(&self) -> CallOptions<'_> {
        self.options.read().await.clone()
    }

    async fn send<R: jacquard_common::types::xrpc::XrpcRequest + Send>(
        self,
        request: &R,
    ) -> XrpcResult<Response<R>> {
        let base_uri = self.base_uri();
        let auth = self.access_token().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = Some(auth);
        let res = self
            .client
            .xrpc(base_uri.clone())
            .with_options(opts.clone())
            .send(request)
            .await;
        if is_invalid_token_response(&res) {
            opts.auth = Some(
                self.refresh()
                    .await
                    .map_err(|e| ClientError::Transport(TransportError::Other(e.into())))?,
            );
            self.client
                .xrpc(base_uri)
                .with_options(opts)
                .send(request)
                .await
        } else {
            res
        }
    }
}

fn is_invalid_token_response<R: XrpcRequest>(response: &XrpcResult<Response<R>>) -> bool {
    match response {
        Err(ClientError::Auth(AuthError::InvalidToken)) => true,
        Err(ClientError::Auth(AuthError::Other(value))) => value
            .to_str()
            .is_ok_and(|s| s.starts_with("DPoP ") && s.contains("error=\"invalid_token\"")),
        _ => false,
    }
}
