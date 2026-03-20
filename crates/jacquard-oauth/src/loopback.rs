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
#![cfg(feature = "loopback")]
use crate::{
    atproto::AtprotoClientMetadata,
    authstore::ClientAuthStore,
    client::OAuthClient,
    dpop::DpopExt,
    error::{CallbackError, OAuthError},
    resolver::OAuthResolver,
    types::{AuthorizeOptions, CallbackParams},
};
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::{IntoStatic, cowstr::ToCowStr};
use rouille::Server;
use std::net::SocketAddr;
use tokio::sync::mpsc;

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

fn create_callback_router(
    request: &rouille::Request,
    tx: mpsc::Sender<CallbackParams>,
) -> rouille::Response {
    rouille::router!(request,
            (GET) (/oauth/callback) => {
                let state = request.get_param("state").unwrap();
                let code = request.get_param("code").unwrap();
                let iss = request.get_param("iss").unwrap();
                let callback_params = CallbackParams {
                    state: Some(state.to_cowstr().into_static()),
                    code: code.to_cowstr().into_static(),
                    iss: Some(iss.to_cowstr().into_static()),
                };
                tx.try_send(callback_params).unwrap();
                rouille::Response::text("Logged in!")
            },
            _ => rouille::Response::empty_404()
    )
}

/// Handle to a running loopback callback server, used to await the OAuth redirect.
pub struct CallbackHandle {
    #[allow(dead_code)]
    server_handle: std::thread::JoinHandle<()>,
    server_stop: std::sync::mpsc::Sender<()>,
    callback_rx: mpsc::Receiver<CallbackParams<'static>>,
}

/// One-shot OAuth callback server.
///
/// Starts an ephemeral in-process web server that listens for the OAuth
/// callback redirect. Returns the server address and a [`CallbackHandle`]
/// that can be used to wait for the callback and stop the server.
///
/// Use in combination with [`handle_localhost_callback`] to handle the
/// callback for the localhost loopback server.
pub fn one_shot_server(addr: SocketAddr) -> (SocketAddr, CallbackHandle) {
    let (tx, callback_rx) = mpsc::channel(5);
    let server = Server::new(addr, move |request| {
        create_callback_router(request, tx.clone())
    })
    .expect("Could not start server");
    let (server_handle, server_stop) = server.stoppable();
    let handle = CallbackHandle {
        server_handle,
        server_stop,
        callback_rx,
    };
    (addr, handle)
}

/// Handles the OAuth callback for the localhost loopback server.
///
/// Returns a session if the callback succeeds within the configured timeout
/// and shuts down the server.
pub async fn handle_localhost_callback<T, S>(
    handle: CallbackHandle,
    flow_client: &super::client::OAuthClient<T, S>,
    cfg: &LoopbackConfig,
) -> crate::error::Result<super::client::OAuthSession<T, S>>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    // Await callback or timeout
    let mut callback_rx = handle.callback_rx;
    let cb = tokio::time::timeout(
        std::time::Duration::from_millis(cfg.timeout_ms),
        callback_rx.recv(),
    )
    .await;
    // trigger shutdown
    let _ = handle.server_stop.send(());
    if let Ok(Some(cb)) = cb {
        // Handle callback and create a session
        Ok(flow_client.callback(cb).await?)
    } else {
        Err(OAuthError::Callback(CallbackError::Timeout))
    }
}

impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    /// Drive the full OAuth flow using a local loopback server.
    ///
    /// This uses localhost OAuth and an ephemeral in-process web server to
    /// handle the OAuth callback redirect. It has a bunch of nice friendly
    /// defaults to help you get started and will basically drive the *entire*
    /// callback flow itself.
    ///
    /// Best used for development and for small CLI applications that don't
    /// require long session lengths. For long-running unattended sessions,
    /// app passwords (via CredentialSession in the jacquard crate) remain
    /// the best option. For more complex OAuth, or if you want more control
    /// over the process, use the other methods on OAuthClient.
    ///
    /// 'input' parameter is what you type in the login box (usually, your handle)
    /// for it to look up your PDS and redirect to its authentication interface.
    ///
    /// If the `browser-open` feature is enabled, this will open a web browser
    /// for you to authenticate with your PDS. It will also print the
    /// callback url to the console for you to copy.
    pub async fn login_with_local_server(
        &self,
        input: impl AsRef<str>,
        opts: AuthorizeOptions<'_>,
        cfg: LoopbackConfig,
    ) -> crate::error::Result<super::client::OAuthSession<T, S>> {
        let port = match cfg.port {
            LoopbackPort::Fixed(p) => p,
            LoopbackPort::Ephemeral => 0,
        };
        // TODO: fix this to it also accepts ipv6 and properly finds a free port
        let bind_addr: SocketAddr = format!("0.0.0.0:{}", port)
            .parse()
            .expect("invalid loopback host/port");
        let (local_addr, handle) = one_shot_server(bind_addr);
        println!("Listening on {}", local_addr);

        let client_data = self.build_localhost_client_data(&cfg, &opts, local_addr);
        // Build client using store and resolver
        let flow_client = OAuthClient::new_with_shared(
            self.registry.store.clone(),
            self.client.clone(),
            client_data,
        );

        // Start auth and get authorization URL
        let auth_url = flow_client.start_auth(input.as_ref(), opts).await?;
        // Print URL for copy/paste
        println!("To authenticate with your PDS, visit:\n{}\n", auth_url);
        // Optionally open browser
        if cfg.open_browser {
            let _ = try_open_in_browser(&auth_url);
        }

        handle_localhost_callback(handle, &flow_client, &cfg).await
    }

    /// Builds a [`crate::session::ClientData`] for use with the local loopback server method of OAuth.
    pub fn build_localhost_client_data(
        &self,
        cfg: &LoopbackConfig,
        opts: &AuthorizeOptions<'_>,
        local_addr: SocketAddr,
    ) -> crate::session::ClientData<'static> {
        let redirect_uri = format!("http://{}:{}/oauth/callback", cfg.host, local_addr.port(),);
        let redirect = Uri::parse(redirect_uri).unwrap();

        let scopes = if opts.scopes.is_empty() {
            Some(self.registry.client_data.config.scopes.clone())
        } else {
            Some(opts.scopes.clone().into_static())
        };

        crate::session::ClientData {
            keyset: self.registry.client_data.keyset.clone(),
            config: AtprotoClientMetadata::new_localhost(Some(vec![redirect]), scopes),
        }
        .into_static()
    }
}
