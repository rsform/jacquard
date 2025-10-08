use std::sync::Arc;

use jacquard_api::com_atproto::server::refresh_session::RefreshSession;
use jacquard_common::{
    AuthorizationToken, CowStr, IntoStatic,
    error::{AuthError, ClientError, XrpcResult},
    http_client::HttpClient,
    session::SessionStore,
    types::{
        did::Did,
        xrpc::{CallOptions, Response, XrpcClient, XrpcError, XrpcExt, XrpcRequest},
    },
};
use tokio::sync::RwLock;
use url::Url;

use crate::client::{AtpSession, token::StoredSession};

pub type SessionKey = (Did<'static>, CowStr<'static>);

pub struct CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
{
    store: Arc<S>,
    client: Arc<T>,
    pub options: RwLock<CallOptions<'static>>,
    pub key: RwLock<Option<SessionKey>>,
    pub endpoint: RwLock<Option<Url>>,
}

impl<S, T> CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession>,
{
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
    pub fn with_options(self, options: CallOptions<'_>) -> Self {
        Self {
            client: self.client,
            store: self.store,
            options: RwLock::new(options.into_static()),
            key: self.key,
            endpoint: self.endpoint,
        }
    }

    pub async fn set_options(&self, options: CallOptions<'_>) {
        *self.options.write().await = options.into_static();
    }

    pub async fn session_info(&self) -> Option<SessionKey> {
        self.key.read().await.clone()
    }

    pub async fn endpoint(&self) -> Url {
        self.endpoint.read().await.clone().unwrap_or(
            Url::parse("https://public.bsky.app").expect("public appview should be valid url"),
        )
    }

    pub async fn set_endpoint(&self, endpoint: Url) {
        *self.endpoint.write().await = Some(endpoint);
    }

    pub async fn access_token(&self) -> Option<AuthorizationToken<'_>> {
        let key = self.key.read().await.clone()?;
        let session = self.store.get(&key).await;
        session.map(|session| AuthorizationToken::Bearer(session.access_jwt))
    }

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
    pub async fn refresh(&self) -> Result<AuthorizationToken<'_>, ClientError> {
        let key = self.key.read().await.clone().ok_or(ClientError::Auth(
            jacquard_common::error::AuthError::NotAuthenticated,
        ))?;
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
            .into_output()
            .map_err(|_| ClientError::Auth(jacquard_common::error::AuthError::RefreshFailed))?;

        let new_session: AtpSession = refresh.into();
        let token = AuthorizationToken::Bearer(new_session.access_jwt.clone());
        self.store
            .set(key, new_session)
            .await
            .map_err(|_| ClientError::Auth(jacquard_common::error::AuthError::RefreshFailed))?;

        Ok(token)
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
        self.endpoint.blocking_read().clone().unwrap_or(
            Url::parse("https://public.bsky.app").expect("public appview should be valid url"),
        )
    }
    async fn send<R: jacquard_common::types::xrpc::XrpcRequest + Send>(
        self,
        request: &R,
    ) -> XrpcResult<Response<R>> {
        let base_uri = self.base_uri();
        let auth = self.access_token().await;
        let mut opts = self.options.read().await.clone();
        opts.auth = auth;
        let resp = self
            .client
            .xrpc(base_uri.clone())
            .with_options(opts.clone())
            .send(request)
            .await;

        if is_expired(&resp) {
            let auth = self.refresh().await?;
            opts.auth = Some(auth);
            self.client
                .xrpc(base_uri)
                .with_options(opts)
                .send(request)
                .await
        } else {
            resp
        }
    }
}

fn is_expired<R: XrpcRequest>(response: &XrpcResult<Response<R>>) -> bool {
    match response {
        Err(ClientError::Auth(AuthError::TokenExpired)) => true,
        Ok(resp) => match resp.parse() {
            Err(XrpcError::Auth(AuthError::TokenExpired)) => true,
            _ => false,
        },
        _ => false,
    }
}
