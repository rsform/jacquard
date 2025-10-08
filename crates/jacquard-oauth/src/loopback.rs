#![cfg(feature = "loopback")]

use crate::{
    atproto::AtprotoClientMetadata,
    authstore::ClientAuthStore,
    client::OAuthClient,
    dpop::DpopExt,
    error::{CallbackError, OAuthError},
    resolver::OAuthResolver,
    scopes::Scope,
    types::{AuthorizeOptions, CallbackParams},
};
use jacquard_common::{IntoStatic, cowstr::ToCowStr};
use rouille::Server;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use url::Url;

#[derive(Clone, Debug)]
pub enum LoopbackPort {
    Fixed(u16),
    Ephemeral,
}

#[derive(Clone, Debug)]
pub struct LoopbackConfig {
    pub host: String,
    pub port: LoopbackPort,
    pub open_browser: bool,
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

#[cfg(feature = "browser-open")]
fn try_open_in_browser(url: &str) -> bool {
    webbrowser::open(url).is_ok()
}
#[cfg(not(feature = "browser-open"))]
fn try_open_in_browser(_url: &str) -> bool {
    false
}

pub fn create_callback_router(
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

struct CallbackHandle {
    #[allow(dead_code)]
    server_handle: std::thread::JoinHandle<()>,
    server_stop: std::sync::mpsc::Sender<()>,
    callback_rx: mpsc::Receiver<CallbackParams<'static>>,
}

fn one_shot_server(addr: SocketAddr) -> (SocketAddr, CallbackHandle) {
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

impl<T, S> OAuthClient<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    /// Drive the full OAuth flow using a local loopback server.
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
        // build redirect uri
        let redirect = Url::parse(&format!(
            "http://{}:{}/oauth/callback",
            cfg.host,
            local_addr.port(),
        ))
        .unwrap();
        let client_data = crate::session::ClientData {
            keyset: self.registry.client_data.keyset.clone(),
            config: AtprotoClientMetadata::new_localhost(
                Some(vec![redirect.clone()]),
                Some(vec![
                    Scope::Atproto,
                    Scope::Transition(crate::scopes::TransitionScope::Generic),
                ]),
            ),
        };

        // Build client using store and resolver
        let flow_client = OAuthClient::new_with_shared(
            self.registry.store.clone(),
            self.client.clone(),
            client_data.clone(),
        );

        // Start auth and get authorization URL
        let auth_url = flow_client.start_auth(input.as_ref(), opts).await?;
        // Print URL for copy/paste
        println!("To authenticate with your PDS, visit:\n{}\n", auth_url);
        // Optionally open browser
        if cfg.open_browser {
            let _ = try_open_in_browser(&auth_url);
        }

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
}
