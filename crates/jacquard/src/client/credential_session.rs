use std::sync::Arc;

use jacquard_api::com_atproto::server::{
    create_session::CreateSession, refresh_session::RefreshSession,
};
use jacquard_common::{
    AuthorizationToken, CowStr, IntoStatic,
    bos::BosStr,
    deps::fluent_uri::Uri,
    error::{AuthError, ClientError, XrpcResult},
    http_client::HttpClient,
    session::SessionStore,
    types::{did::Did, string::Handle},
    xrpc::{CallOptions, Response, XrpcClient, XrpcExt, XrpcRequest, XrpcResp, XrpcResponse},
};
#[cfg(feature = "streaming")]
use serde::Serialize;
use smol_str::SmolStr;
use tokio::sync::RwLock;

use crate::client::AtpSession;
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use std::any::Any;

#[cfg(feature = "websocket")]
use jacquard_common::websocket::{WebSocketClient, WebSocketConnection};
#[cfg(feature = "websocket")]
use jacquard_common::xrpc::XrpcSubscription;

/// Storage key for app‑password sessions: `(account DID, session id)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey(pub Did, pub SmolStr);

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
    pub options: RwLock<CallOptions<'static>>,
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
    pub fn with_options(self, options: CallOptions<'_>) -> Self {
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
    pub async fn set_options(&self, options: CallOptions<'_>) {
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
        opts.auth = session.map(|s| AuthorizationToken::Bearer(s.refresh_jwt));
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

        let new_session: AtpSession = refresh.into();
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
        identifier: CowStr<'_>,
        password: CowStr<'_>,
        session_id: Option<CowStr<'_>>,
        allow_takendown: Option<bool>,
        auth_factor_token: Option<CowStr<'_>>,
        pds: Option<Uri<String>>,
    ) -> std::result::Result<AtpSession, ClientError>
    where
        S: Any + 'static,
    {
        #[cfg(feature = "tracing")]
        let _span =
            tracing::info_span!("credential_session_login", identifier = %identifier).entered();

        // Resolve PDS base
        let pds = if let Some(pds) = pds {
            pds
        } else if identifier.as_ref().starts_with("http://")
            || identifier.as_ref().starts_with("https://")
        {
            Uri::parse(identifier.as_ref())
                .map_err(|e| {
                    ClientError::from(e)
                        .with_help("identifier should be a valid https:// URL, handle, or DID")
                })?
                .to_owned()
        } else if identifier.as_ref().starts_with("did:") {
            let did = Did::new(identifier.as_ref()).map_err(|e| {
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
        } else if identifier.as_ref().contains("@") && !identifier.as_ref().starts_with("@") {
            // we're going to assume its an email
            pds.ok_or_else(|| {
                ClientError::invalid_request("missing PDS endpoint")
                    .with_help("When logging in with email, we need your PDS")
            })?
        } else {
            // treat as handle
            let handle =
                jacquard_common::types::string::Handle::new(identifier.as_ref()).map_err(|e| {
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
            auth_factor_token,
            identifier: identifier.clone().into_static(),
            password: password.into_static(),
            extra_data: None,
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
        let session = AtpSession::from(out);

        let sid = session_id.unwrap_or_else(|| CowStr::new_static("session"));
        let key = SessionKey(session.did.clone().convert::<SmolStr>(), SmolStr::from(sid));
        self.store
            .set(key.clone(), session.clone())
            .await
            .map_err(|e| ClientError::from(e).with_context("failed to persist session to store"))?;
        // If using FileAuthStore, persist PDS for faster resume
        if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            let _ = file_store.set_atp_pds(&key, &pds);
        }
        // Activate
        *self.key.write().await = Some(key);
        let pds_uri = jacquard_common::xrpc::normalize_base_uri(pds);
        *self.endpoint.write().await = Some(pds_uri);

        Ok(session)
    }

    /// Restore a previously persisted app-password session and set base endpoint.
    pub async fn restore(
        &self,
        did: Did,
        session_id: CowStr<'_>,
    ) -> std::result::Result<(), ClientError>
    where
        S: Any + 'static,
    {
        #[cfg(feature = "tracing")]
        let _span =
            tracing::info_span!("credential_session_restore", did = %did, session_id = %session_id)
                .entered();

        let key = SessionKey(did.clone(), SmolStr::from(session_id.clone()));
        let Some(sess) = self.store.get(&key).await else {
            return Err(ClientError::auth(AuthError::NotAuthenticated));
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
            let resp = self.client.resolve_did_doc(&did).await?;
            resp.into_owned()?
                .pds_endpoint()
                .map(|u| u.to_owned())
                .ok_or_else(|| {
                    ClientError::invalid_request("missing PDS endpoint")
                        .with_help("DID document must include a PDS service endpoint")
                })?
        });

        // Activate
        *self.key.write().await = Some(key.clone());
        let pds_uri = jacquard_common::xrpc::normalize_base_uri(pds);
        *self.endpoint.write().await = Some(pds_uri.clone());
        // ensure store has the session (no-op if it existed)
        self.store
            .set(
                SessionKey(
                    sess.did.clone().convert::<SmolStr>(),
                    SmolStr::from(session_id),
                ),
                sess,
            )
            .await?;
        if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            let _ = file_store.set_atp_pds(&key, &pds_uri);
        }
        Ok(())
    }

    /// Switch to a different stored session (and refresh endpoint/PDS).
    pub async fn switch_session(
        &self,
        did: Did,
        session_id: CowStr<'_>,
    ) -> std::result::Result<(), ClientError>
    where
        S: Any + 'static,
    {
        let key = SessionKey(did.clone(), SmolStr::from(session_id));
        if self.store.get(&key).await.is_none() {
            return Err(ClientError::auth(AuthError::NotAuthenticated));
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
            let resp = self.client.resolve_did_doc(&did).await?;
            resp.into_owned()?
                .pds_endpoint()
                .map(|u| u.to_owned())
                .ok_or_else(|| {
                    ClientError::invalid_request("missing PDS endpoint")
                        .with_help("DID document must include a PDS service endpoint")
                })?
        });
        *self.key.write().await = Some(key.clone());
        let pds_uri = jacquard_common::xrpc::normalize_base_uri(pds);
        *self.endpoint.write().await = Some(pds_uri.clone());
        if let Some(file_store) =
            (&*self.store as &dyn Any).downcast_ref::<crate::client::token::FileAuthStore>()
        {
            let _ = file_store.set_atp_pds(&key, &pds_uri);
        }
        Ok(())
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
        R: XrpcRequest + Send + Sync + serde::Serialize,
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
                AuthorizationToken::Dpop(t) => {
                    http::HeaderValue::from_str(&format!("DPoP {}", t.as_str()))
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
                    AuthorizationToken::Dpop(t) => {
                        http::HeaderValue::from_str(&format!("DPoP {}", t.as_str()))
                    }
                }
                .map_err(|e| {
                    StreamError::protocol(format!("Invalid authorization token: {}", e))
                })?;
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
                AuthorizationToken::Dpop(t) => format!("DPoP {}", t.as_str()),
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
