use bytes::Bytes;
use jacquard_common::{
    AuthorizationToken, IntoStatic,
    error::{AuthError, ClientError, HttpError, TransportError, XrpcResult},
    http_client::HttpClient,
    session::{SessionStore, SessionStoreError},
    types::{
        did::Did,
        xrpc::{CallOptions, Response, XrpcExt},
    },
};
use url::Url;

use jacquard_common::types::xrpc::{XrpcRequest, build_http_request};

use crate::client::{AtpSession, AuthSession, FileTokenStore, NSID_REFRESH_SESSION};

/// Per-call overrides when sending via `AtClient`.
#[derive(Debug, Default, Clone)]
pub struct SendOverrides<'a> {
    pub did: Option<Did<'a>>,
    /// Optional base URI override for this call.
    pub base_uri: Option<Url>,
    /// Per-request options such as auth, proxy, labelers, extra headers.
    pub options: CallOptions<'a>,
    /// Whether to auto-refresh on expired/invalid token and retry once.
    pub auto_refresh: bool,
}

impl<'a> SendOverrides<'a> {
    /// Construct default overrides (no base override, auto-refresh enabled).
    pub fn new() -> Self {
        Self {
            did: None,
            base_uri: None,
            options: CallOptions::default(),
            auto_refresh: true,
        }
    }
    /// Override the base URI for this call only.
    pub fn base_uri(mut self, base: Url) -> Self {
        self.base_uri = Some(base);
        self
    }
    /// Provide a full set of call options (auth/headers/etc.).
    pub fn options(mut self, opts: CallOptions<'a>) -> Self {
        self.options = opts;
        self
    }

    /// Provide a full set of call options (auth/headers/etc.).
    pub fn did(mut self, did: Did<'a>) -> Self {
        self.did = Some(did);
        self
    }
    /// Enable or disable one-shot auto-refresh + retry behavior.
    pub fn auto_refresh(mut self, enable: bool) -> Self {
        self.auto_refresh = enable;
        self
    }
}

/// Stateful client for AT Protocol XRPC with token storage and auto-refresh.
///
/// Example (file-backed tokens)
/// ```ignore
/// use jacquard::client::{AtClient, FileTokenStore, TokenStore};
/// use jacquard::api::com_atproto::server::create_session::CreateSession;
/// use jacquard::client::AtClient as _; // method resolution
/// use jacquard::CowStr;
///
/// #[tokio::main]
/// async fn main() -> miette::Result<()> {
///     let base = url::Url::parse("https://bsky.social")?;
///     let store = FileTokenStore::new("/tmp/jacquard-session.json");
///     let client = AtClient::new(reqwest::Client::new(), base, store);
///     let session = client
///         .send(
///             CreateSession::new()
///                 .identifier(CowStr::from("alice.example"))
///                 .password(CowStr::from("app-password"))
///                 .build(),
///         )
///         .await?
///         .into_output()?;
///     client.set_session(session.into()).await?;
///     Ok(())
/// }
/// ```
pub struct AtClient<C: HttpClient, S> {
    transport: C,
    base: Url,
    tokens: S,
    refresh_lock: tokio::sync::Mutex<Option<Did<'static>>>,
}

impl<C: HttpClient, S: SessionStore<Did<'static>, AuthSession>> AtClient<C, S> {
    /// Create a new client with a transport, base URL, and token store.
    pub fn new(transport: C, base: Url, tokens: S) -> Self {
        Self {
            transport,
            base,
            tokens,
            refresh_lock: tokio::sync::Mutex::new(None),
        }
    }

    /// Get the base URL of this client.
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Access the underlying transport.
    pub fn transport(&self) -> &C {
        &self.transport
    }

    /// Get the current session, if any.
    pub async fn session(&self, did: &Did<'static>) -> Option<AuthSession> {
        self.tokens.get(did).await
    }

    /// Set the current session in the token store.
    pub async fn set_session(&self, session: AuthSession) -> Result<(), SessionStoreError> {
        let s = session.clone();
        let did = s.did().clone().into_static();
        self.refresh_lock.lock().await.replace(did.clone());
        self.tokens.set(did, session).await
    }

    /// Clear the current session from the token store.
    pub async fn clear_session(&self) -> Result<(), SessionStoreError> {
        self.tokens.clear().await
    }

    /// Send an XRPC request using the client's base URL and default behavior.
    pub async fn send<R: XrpcRequest + Send>(&self, req: R) -> XrpcResult<Response<R>> {
        self.send_with(req, SendOverrides::new()).await
    }

    /// Send an XRPC request with per-call overrides.
    pub async fn send_with<R: XrpcRequest + Send>(
        &self,
        req: R,
        mut overrides: SendOverrides<'_>,
    ) -> XrpcResult<Response<R>> {
        let base = overrides
            .base_uri
            .clone()
            .unwrap_or_else(|| self.base.clone());
        let is_refresh = R::NSID == NSID_REFRESH_SESSION;

        let mut current_did = None;
        if overrides.options.auth.is_none() {
            if let Ok(guard) = self.refresh_lock.try_lock() {
                if let Some(ref did) = *guard {
                    current_did = Some(did.clone());
                    if let Some(s) = self.tokens.get(&did).await {
                        overrides.options.auth = Some(
                            if let Some(refresh_tok) = s.refresh_token()
                                && is_refresh
                            {
                                AuthorizationToken::Bearer(refresh_tok.clone().into_static())
                            } else {
                                AuthorizationToken::Bearer(s.access_token().clone().into_static())
                            },
                        );
                    }
                }
            }
        }

        let http_request =
            build_http_request(&base, &req, &overrides.options).map_err(TransportError::from)?;
        let http_response = self
            .transport
            .send_http(http_request)
            .await
            .map_err(|e| TransportError::Other(Box::new(e)))?;
        let status = http_response.status();
        let buffer = Bytes::from(http_response.into_body());

        if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
            return Err(HttpError {
                status,
                body: Some(buffer),
            }
            .into());
        }

        if overrides.auto_refresh
            && !is_refresh
            && overrides.options.auth.is_some()
            && Self::is_auth_expired(status, &buffer)
        {
            self.refresh_once().await?;

            let mut retry_opts = overrides.options.clone();
            if let Some(curr_did) = current_did {
                if let Some(s) = self.tokens.get(&curr_did).await {
                    retry_opts.auth = Some(AuthorizationToken::Bearer(
                        s.access_token().clone().into_static(),
                    ));
                }
            }
            let http_request =
                build_http_request(&base, &req, &retry_opts).map_err(TransportError::from)?;
            let http_response = self
                .transport
                .send_http(http_request)
                .await
                .map_err(|e| TransportError::Other(Box::new(e)))?;
            let status = http_response.status();
            let buffer = Bytes::from(http_response.into_body());

            if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
                return Err(HttpError {
                    status,
                    body: Some(buffer),
                }
                .into());
            }
            return Ok(Response::new(buffer, status));
        }

        Ok(Response::new(buffer, status))
    }

    async fn refresh_once(&self) -> XrpcResult<()> {
        let guard = self.refresh_lock.lock().await;
        if let Some(ref did) = *guard {
            if let Some(s) = self.tokens.get(did).await {
                if let Some(refresh_tok) = s.refresh_token() {
                    let refresh_resp = self
                        .transport
                        .xrpc(self.base.clone())
                        .auth(AuthorizationToken::Bearer(
                            refresh_tok.clone().into_static(),
                        ))
                        .send(jacquard_api::com_atproto::server::refresh_session::RefreshSession)
                        .await?;
                    let refreshed = match refresh_resp.into_output() {
                        Ok(o) => AtpSession::from(o),
                        Err(_) => return Err(ClientError::Auth(AuthError::RefreshFailed)),
                    };

                    let mut session = s.clone();
                    session.set_access_token(refreshed.access_jwt);
                    session.set_refresh_token(refreshed.refresh_jwt);

                    self.set_session(session)
                        .await
                        .map_err(|_| ClientError::Auth(AuthError::RefreshFailed))?;
                    Ok(())
                } else {
                    Err(ClientError::Auth(AuthError::RefreshFailed))
                }
            } else {
                Err(ClientError::Auth(AuthError::NotAuthenticated))
            }
        } else {
            Err(ClientError::Auth(AuthError::NotAuthenticated))
        }
    }

    fn is_auth_expired(status: http::StatusCode, buffer: &Bytes) -> bool {
        if status.as_u16() == 401 {
            return true;
        }
        if status.as_u16() == 400 {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(buffer) {
                if let Some(code) = val.get("error").and_then(|v| v.as_str()) {
                    return matches!(code, "ExpiredToken" | "InvalidToken");
                }
            }
        }
        false
    }
}
