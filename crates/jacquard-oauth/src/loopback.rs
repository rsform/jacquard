//!
//! Helpers for the local loopback server method of atproto OAuth.
//!
//! `OAuthClient::login_with_local_server()` is the nice helper. Here is where
//! it and its components live. Below is what it does, so you can have more
//! granular control without having to make your own loopback server.
//!
//! ```ignore
//! let input = "your_handle_here";
//! let cfg = LoopbackConfig::default();
//! let opts = AuthorizeOptions::default();
//! let port = match cfg.port {
//!     LoopbackPort::Fixed(p) => p,
//!     LoopbackPort::Ephemeral => 0,
//! };
//! // TODO: fix this to it also accepts ipv6 and properly finds a free port
//! let bind_addr: SocketAddr = format!("0.0.0.0:{}", port)
//!     .parse()
//!     .expect("invalid loopback host/port");
//! let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
//!
//! let (local_addr, handle) = one_shot_server(bind_addr);
//! println!("Listening on {}", local_addr);
//!
//! let client_data = oauth.build_localhost_client_data(&cfg, &opts, local_addr);
//! // Build client using store and resolver
//! let flow_client = OAuthClient::new_with_shared(
//!     self.registry.store.clone(),
//!     self.client.clone(),
//!     client_data,
//! );
//!
//! // Start auth and get authorization URL
//! let auth_url = flow_client.start_auth(input.as_ref(), opts).await?;
//! // Print URL for copy/paste
//! println!("To authenticate with your PDS, visit:\n{}\n", auth_url);
//! // Optionally open browser
//! if cfg.open_browser {
//!     let _ = try_open_in_browser(&auth_url);
//! }
//!
//! handle_localhost_callback(handle, &flow_client, &cfg).await
//! ```
//!
//!
#![cfg(all(
    feature = "loopback",
    not(all(target_arch = "wasm32", target_os = "unknown"))
))]
use crate::{
    atproto::AtprotoClientMetadata,
    authstore::{ClientAuthStore, OAuthSessionMatch},
    client::OAuthClient,
    dpop::DpopExt,
    error::{CallbackError, OAuthError},
    resolver::OAuthResolver,
    types::{AuthorizeOptions, CallbackParams},
};
use jacquard_common::IntoStatic;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::session::{SessionHint, SessionSelector, SessionStoreError};
use jacquard_common::types::{did::Did, string::Handle};
use smol_str::SmolStr;
use std::net::SocketAddr;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{mpsc, oneshot},
};

fn oauth_hint_from_input(input: &str) -> SessionHint<SmolStr> {
    if let Ok(did) = Did::new(input) {
        SessionHint::Did(did.convert())
    } else if let Ok(handle) = Handle::new(input) {
        SessionHint::Handle(handle.convert())
    } else {
        SessionHint::Identifier(SmolStr::from(input))
    }
}

/// Port selection strategy for the loopback OAuth callback server.
#[derive(Clone, Debug)]
pub enum LoopbackPort {
    /// Bind to a specific port number.
    Fixed(u16),
    /// Let the OS assign an available port.
    Ephemeral,
}

/// Configuration for the loopback OAuth callback server.
#[derive(Clone, Debug)]
pub struct LoopbackConfig {
    /// The host address to bind to (e.g., `"127.0.0.1"`).
    pub host: String,
    /// Port selection strategy.
    pub port: LoopbackPort,
    /// Whether to attempt opening the authorization URL in the user's browser.
    pub open_browser: bool,
    /// How long to wait for the callback before timing out, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for LoopbackConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: LoopbackPort::Fixed(4000),
            open_browser: true,
            timeout_ms: 5 * 60 * 1000,
        }
    }
}

/// Attempts to open the given URL in the user's default browser.
///
/// Returns `true` if the browser was opened successfully, `false` otherwise.
#[cfg(feature = "browser-open")]
pub fn try_open_in_browser(url: &str) -> bool {
    webbrowser::open(url).is_ok()
}
/// Stub for when the `browser-open` feature is disabled. Always returns `false`.
#[cfg(not(feature = "browser-open"))]
pub fn try_open_in_browser(_url: &str) -> bool {
    false
}

async fn handle_callback_connection(mut stream: TcpStream, tx: mpsc::Sender<CallbackParams>) {
    let Some(Some(params)) = read_callback_params(&mut stream).await else {
        let _ = write_http_response(&mut stream, 404, "Not found").await;
        return;
    };

    match tx.try_send(params) {
        Ok(()) => {
            let _ = write_http_response(&mut stream, 200, "Logged in!").await;
        }
        Err(_) => {
            let _ = write_http_response(&mut stream, 500, "Could not deliver OAuth callback").await;
        }
    }
}

async fn read_callback_params(stream: &mut TcpStream) -> Option<Option<CallbackParams>> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await.ok()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    if method != "GET" {
        return Some(None);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/oauth/callback" {
        return Some(None);
    }
    serde_html_form::from_str(query).ok().map(Some)
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await
}

/// Handle to a running loopback callback server, used to await the OAuth redirect.
pub struct CallbackHandle {
    server_handle: tokio::task::JoinHandle<()>,
    server_stop: oneshot::Sender<()>,
    callback_rx: mpsc::Receiver<CallbackParams>,
}

/// One-shot OAuth callback server.
///
/// Starts an ephemeral in-process web server that listens for the OAuth
/// callback redirect. Returns the server address and a [`CallbackHandle`]
/// that can be used to wait for the callback and stop the server.
///
/// Use in combination with [`handle_localhost_callback`] to handle the
/// callback for the localhost loopback server.
pub async fn one_shot_server(
    addr: impl ToSocketAddrs,
) -> std::io::Result<(SocketAddr, CallbackHandle)> {
    let (tx, callback_rx) = mpsc::channel(5);
    let listener = TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    let (server_stop, mut stop_rx) = oneshot::channel();
    let server_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, _)) => {
                            tokio::spawn(handle_callback_connection(stream, tx.clone()));
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });
    let handle = CallbackHandle {
        server_handle,
        server_stop,
        callback_rx,
    };
    Ok((local_addr, handle))
}

async fn wait_for_callback(
    handle: CallbackHandle,
    timeout_ms: u64,
) -> Result<CallbackParams, OAuthError> {
    let CallbackHandle {
        server_handle,
        server_stop,
        mut callback_rx,
    } = handle;
    let cb = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        callback_rx.recv(),
    )
    .await;
    let _ = server_stop.send(());
    let _ = server_handle.await;
    if let Ok(Some(cb)) = cb {
        Ok(cb)
    } else {
        Err(OAuthError::Callback(CallbackError::Timeout))
    }
}

/// Handles the OAuth callback for the localhost loopback server.
///
/// Returns a session if the callback succeeds within the configured timeout
/// and shuts down the server.
///
/// When the `scope-check` feature is enabled, `T` must also implement `LexiconSchemaResolver`
/// for eager resolution of include scopes.
#[cfg(not(feature = "scope-check"))]
pub async fn handle_localhost_callback<T, S>(
    handle: CallbackHandle,
    flow_client: &super::client::OAuthClient<T, S>,
    cfg: &LoopbackConfig,
) -> crate::error::Result<super::client::OAuthSession<T, S>>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    Ok(flow_client
        .callback(wait_for_callback(handle, cfg.timeout_ms).await?)
        .await?)
}

/// Handles the OAuth callback for the localhost loopback server.
///
/// Returns a session if the callback succeeds within the configured timeout
/// and shuts down the server.
///
/// When the `scope-check` feature is enabled, `T` must also implement `LexiconSchemaResolver`
/// for eager resolution of include scopes.
#[cfg(feature = "scope-check")]
pub async fn handle_localhost_callback<T, S>(
    handle: CallbackHandle,
    flow_client: &super::client::OAuthClient<T, S>,
    cfg: &LoopbackConfig,
) -> crate::error::Result<super::client::OAuthSession<T, S>>
where
    T: OAuthResolver
        + DpopExt
        + jacquard_identity::lexicon_resolver::LexiconSchemaResolver
        + Send
        + Sync
        + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    Ok(flow_client
        .callback(wait_for_callback(handle, cfg.timeout_ms).await?)
        .await?)
}

fn loopback_port(cfg: &LoopbackConfig) -> u16 {
    match cfg.port {
        LoopbackPort::Fixed(port) => port,
        LoopbackPort::Ephemeral => 0,
    }
}

fn redirect_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    async fn start_loopback_flow(
        &self,
        input: &str,
        opts: AuthorizeOptions<SmolStr>,
        cfg: &LoopbackConfig,
    ) -> crate::error::Result<(OAuthClient<T, S>, CallbackHandle)> {
        let (local_addr, handle) = one_shot_server((cfg.host.as_str(), loopback_port(cfg)))
            .await
            .map_err(|err| OAuthError::Callback(CallbackError::LoopbackServer(err.to_string())))?;
        println!("Listening on {}", local_addr);

        let client_data = self.build_localhost_client_data(cfg, &opts, local_addr);
        let flow_client = OAuthClient::new_with_shared(
            self.registry.store.clone(),
            self.client.clone(),
            client_data,
        );

        let auth_url = flow_client.start_auth(input, opts).await?;
        println!("To authenticate with your PDS, visit:\n{}\n", auth_url);
        if cfg.open_browser {
            let _ = try_open_in_browser(&auth_url);
        }

        Ok((flow_client, handle))
    }

    /// Builds a [`crate::session::ClientData`] for use with the local loopback server method of OAuth.
    pub fn build_localhost_client_data(
        &self,
        cfg: &LoopbackConfig,
        opts: &AuthorizeOptions<SmolStr>,
        local_addr: SocketAddr,
    ) -> crate::session::ClientData<SmolStr> {
        let redirect_uri = format!(
            "http://{}:{}/oauth/callback",
            redirect_host(&cfg.host),
            local_addr.port(),
        );
        let redirect = Uri::parse(redirect_uri).unwrap();

        let scopes = if opts.scopes.is_empty() {
            Some(self.registry.client_data.config.scopes.clone())
        } else {
            Some(opts.scopes.clone())
        };

        crate::session::ClientData {
            keyset: self.registry.client_data.keyset.clone(),
            config: AtprotoClientMetadata::new_localhost(Some(vec![redirect]), scopes),
        }
        .into_static()
    }

    async fn restore_matching_session(
        &self,
        input: &str,
    ) -> crate::error::Result<Option<super::client::OAuthSession<T, S>>>
    where
        S: SessionSelector<OAuthSessionMatch, Error = SessionStoreError>,
    {
        let hint = oauth_hint_from_input(input);
        if let Some(matched) = self.registry.store.select_session(&hint).await? {
            Ok(Some(
                self.restore(&matched.key.did, matched.key.session_id.as_str())
                    .await?,
            ))
        } else {
            Ok(None)
        }
    }
}

#[cfg(not(feature = "scope-check"))]
impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    /// Drive the full OAuth flow using a local loopback server.
    ///
    /// This uses localhost OAuth and an ephemeral in-process web server to
    /// handle the OAuth callback redirect. It has friendly defaults to drive
    /// the entire callback flow for development and small CLI applications.
    pub async fn login_with_local_server(
        &self,
        input: impl AsRef<str>,
        opts: AuthorizeOptions<SmolStr>,
        cfg: LoopbackConfig,
    ) -> crate::error::Result<super::client::OAuthSession<T, S>> {
        let (flow_client, handle) = self.start_loopback_flow(input.as_ref(), opts, &cfg).await?;
        handle_localhost_callback(handle, &flow_client, &cfg).await
    }

    /// Resume a stored session for the input identity, or drive the full OAuth flow using a local loopback server.
    pub async fn resume_or_login_with_local_server(
        &self,
        input: impl AsRef<str>,
        opts: AuthorizeOptions<SmolStr>,
        cfg: LoopbackConfig,
    ) -> crate::error::Result<super::client::OAuthSession<T, S>>
    where
        S: SessionSelector<OAuthSessionMatch, Error = SessionStoreError>,
    {
        let input_ref = input.as_ref();
        if let Some(session) = self.restore_matching_session(input_ref).await? {
            return Ok(session);
        }
        self.login_with_local_server(input_ref, opts, cfg).await
    }
}

#[cfg(feature = "scope-check")]
impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver
        + DpopExt
        + jacquard_identity::lexicon_resolver::LexiconSchemaResolver
        + Send
        + Sync
        + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    /// Drive the full OAuth flow using a local loopback server.
    ///
    /// This uses localhost OAuth and an ephemeral in-process web server to
    /// handle the OAuth callback redirect. It has friendly defaults to drive
    /// the entire callback flow for development and small CLI applications.
    pub async fn login_with_local_server(
        &self,
        input: impl AsRef<str>,
        opts: AuthorizeOptions<SmolStr>,
        cfg: LoopbackConfig,
    ) -> crate::error::Result<super::client::OAuthSession<T, S>> {
        let (flow_client, handle) = self.start_loopback_flow(input.as_ref(), opts, &cfg).await?;
        handle_localhost_callback(handle, &flow_client, &cfg).await
    }

    /// Resume a stored session for the input identity, or drive the full OAuth flow using a local loopback server.
    pub async fn resume_or_login_with_local_server(
        &self,
        input: impl AsRef<str>,
        opts: AuthorizeOptions<SmolStr>,
        cfg: LoopbackConfig,
    ) -> crate::error::Result<super::client::OAuthSession<T, S>>
    where
        S: SessionSelector<OAuthSessionMatch, Error = SessionStoreError>,
    {
        let input_ref = input.as_ref();
        if let Some(session) = self.restore_matching_session(input_ref).await? {
            return Ok(session);
        }
        self.login_with_local_server(input_ref, opts, cfg).await
    }
}
