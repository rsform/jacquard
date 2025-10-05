use bytes::Bytes;
use url::Url;

use crate::client::xrpc_call::{CallOptions, XrpcExt};
use crate::client::{self as super_mod, AuthorizationToken, HttpClient, Response, Session, error};
use jacquard_common::types::xrpc::XrpcRequest;

use super::token::TokenStore;

/// Per-call overrides when sending via `AtClient`.
#[derive(Debug, Default, Clone)]
pub struct SendOverrides<'a> {
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
pub struct AtClient<C: HttpClient, S: TokenStore> {
    transport: C,
    base: Url,
    tokens: S,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl<C: HttpClient, S: TokenStore> AtClient<C, S> {
    /// Create a new client with a transport, base URL, and token store.
    pub fn new(transport: C, base: Url, tokens: S) -> Self {
        Self {
            transport,
            base,
            tokens,
            refresh_lock: tokio::sync::Mutex::new(()),
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
    pub async fn session(&self) -> Option<Session> {
        self.tokens.get().await
    }

    /// Set the current session in the token store.
    pub async fn set_session(&self, session: Session) -> Result<(), super_mod::TokenStoreError> {
        self.tokens.set(session).await
    }

    /// Clear the current session from the token store.
    pub async fn clear_session(&self) -> Result<(), super_mod::TokenStoreError> {
        self.tokens.clear().await
    }

    /// Send an XRPC request using the client's base URL and default behavior.
    pub async fn send<R: XrpcRequest + Send>(&self, req: R) -> super_mod::Result<Response<R>> {
        self.send_with(req, SendOverrides::new()).await
    }

    /// Send an XRPC request with per-call overrides.
    pub async fn send_with<R: XrpcRequest + Send>(
        &self,
        req: R,
        mut overrides: SendOverrides<'_>,
    ) -> super_mod::Result<Response<R>> {
        let base = overrides
            .base_uri
            .clone()
            .unwrap_or_else(|| self.base.clone());
        let is_refresh = R::NSID == super_mod::NSID_REFRESH_SESSION;

        if overrides.options.auth.is_none() {
            if let Some(s) = self.tokens.get().await {
                overrides.options.auth = Some(if is_refresh {
                    AuthorizationToken::Bearer(s.refresh_jwt)
                } else {
                    AuthorizationToken::Bearer(s.access_jwt)
                });
            }
        }

        let http_request = super_mod::build_http_request(&base, &req, &overrides.options)
            .map_err(error::TransportError::from)?;
        let http_response = self
            .transport
            .send_http(http_request)
            .await
            .map_err(|e| error::TransportError::Other(Box::new(e)))?;
        let status = http_response.status();
        let buffer = Bytes::from(http_response.into_body());

        if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
            return Err(error::HttpError {
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
            if let Some(s) = self.tokens.get().await {
                retry_opts.auth = Some(AuthorizationToken::Bearer(s.access_jwt));
            }
            let http_request = super_mod::build_http_request(&base, &req, &retry_opts)
                .map_err(error::TransportError::from)?;
            let http_response = self
                .transport
                .send_http(http_request)
                .await
                .map_err(|e| error::TransportError::Other(Box::new(e)))?;
            let status = http_response.status();
            let buffer = Bytes::from(http_response.into_body());

            if !status.is_success() && !matches!(status.as_u16(), 400 | 401) {
                return Err(error::HttpError {
                    status,
                    body: Some(buffer),
                }
                .into());
            }
            return Ok(Response::new(buffer, status));
        }

        Ok(Response::new(buffer, status))
    }

    async fn refresh_once(&self) -> super_mod::Result<()> {
        let _guard = self.refresh_lock.lock().await;
        let Some(s) = self.tokens.get().await else {
            return Err(error::ClientError::Auth(error::AuthError::NotAuthenticated));
        };
        let refresh_token = s.refresh_jwt.clone();
        let refresh_resp = self
            .transport
            .xrpc(self.base.clone())
            .auth(AuthorizationToken::Bearer(refresh_token))
            .send(jacquard_api::com_atproto::server::refresh_session::RefreshSession)
            .await?;
        let refreshed = match refresh_resp.into_output() {
            Ok(o) => Session::from(o),
            Err(_) => return Err(error::ClientError::Auth(error::AuthError::RefreshFailed)),
        };
        self.tokens
            .set(refreshed)
            .await
            .map_err(|_| error::ClientError::Auth(error::AuthError::RefreshFailed))?;
        Ok(())
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
