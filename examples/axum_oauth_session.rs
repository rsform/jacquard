//! Hosted, server-rendered Axum OAuth example.
//!
//! This example demonstrates a web OAuth client whose `client_id` is the public
//! metadata URL served by the same Axum application:
//!
//! ```text
//! https://example.com/oauth-client-metadata.json
//! ```
//!
//! Run it with a public HTTPS origin that reaches this server. For local
//! development, point a domain you control at your machine with something like
//! Tailscale Funnel or a Cloudflare Tunnel. Alternatively, run it on a server
//! with a publicly routable IP address and configured A and/or AAAA records.
//!
//! ```text
//! cargo run -p jacquard-axum --example axum_oauth_server -- \
//!   --base-url https://example.com \
//!   --listen 127.0.0.1:3000
//! ```
//!
//! The example stores generated development secrets under `--data-dir`: the
//! private-cookie key, the OAuth confidential-client keyset, and the file-backed
//! OAuth session store. The default lives under `/tmp` so running the example
//! does not contaminate the repository checkout. For longer-lived local state,
//! use an os/environment appropriate config/state directory. A real
//! deployment should store cookie keys and OAuth signing keys in appropriately
//! protected secret storage, such as an OS keyring or deployment secret manager,
//! with permissions, backups, rotation, access controls, and operational practices
//! chosen for that environment, and would also use a database-backed impl of `ClientAuthStore`.

use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{FromRef, Query},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
};
use axum_extra::extract::cookie::Key;
use clap::Parser;
use html_escape::{encode_double_quoted_attribute, encode_text};
use jacquard::{
    api::app_bsky::feed::get_timeline::GetTimeline,
    client::{Agent, FileAuthStore},
    common::deps::{fluent_uri::Uri, smol_str::SmolStr},
    oauth::{
        atproto::{AtprotoClientMetadata, GrantType},
        client::OAuthClient,
        keyset::Keyset,
        scopes::Scopes,
        session::ClientData,
    },
    xrpc::XrpcClient,
};
use jacquard_axum::oauth::{
    BrowserOAuthSession, ExtractOAuthSession, OAuthWebConfig, OAuthWebState, routes as oauth_routes,
};
use jacquard_identity::PublicResolver;
use miette::{Context, IntoDiagnostic, Result, bail, miette};
use rand::{RngCore, rngs::OsRng};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
struct Args {
    /// Public HTTPS origin for this hosted OAuth client, e.g. `https://example.com`.
    ///
    /// This must be externally reachable by users' PDSes. For local development,
    /// use a tunnel such as Tailscale Funnel or Cloudflare Tunnel, or run on a
    /// server with public DNS A and/or AAAA records.
    #[arg(long)]
    base_url: String,

    /// Directory for this example's generated development state.
    ///
    /// This directory contains secret key material. The default is under `/tmp`
    /// to avoid writing into the repository. For longer-lived local state, prefer
    /// an XDG state/config directory or the platform equivalent; production apps
    /// should use secure secret storage or an OS keyring.
    #[arg(long, default_value = "/tmp/jacquard-axum-oauth-example")]
    data_dir: PathBuf,

    /// Socket address to bind locally.
    #[arg(long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,
}

#[derive(Clone)]
struct AppState {
    oauth: Arc<OAuthClient<PublicResolver, FileAuthStore>>,
    oauth_config: OAuthWebConfig,
    cookie_key: Key,
}

impl OAuthWebState<PublicResolver, FileAuthStore> for AppState {
    fn oauth_client(&self) -> &OAuthClient<PublicResolver, FileAuthStore> {
        self.oauth.as_ref()
    }
}

impl FromRef<AppState> for OAuthWebConfig {
    fn from_ref(input: &AppState) -> Self {
        input.oauth_config.clone()
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(input: &AppState) -> Self {
        input.cookie_key.clone()
    }
}

#[derive(Debug, Deserialize)]
struct LoginQuery {
    #[serde(default)]
    return_to: Option<String>,
}

async fn login_page(Query(query): Query<LoginQuery>) -> Html<String> {
    let return_to = query.return_to.unwrap_or_else(|| "/timeline".to_owned());
    Html(format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Sign in with AT Protocol</title>
</head>
<body>
  <main>
    <h1>Sign in with AT Protocol</h1>
    <p>Enter your handle, DID, or PDS URL. The server will resolve it and start
    the OAuth flow for the matching PDS.</p>
    <form method="get" action="/oauth/start">
      <p>
        <label for="identifier">Handle, DID, or PDS URL</label><br>
        <input id="identifier" name="identifier" autocomplete="username"
          placeholder="alice.bsky.social" required>
      </p>
      <input type="hidden" name="return_to" value="{}">
      <button type="submit">Sign in</button>
    </form>
  </main>
</body>
</html>"#,
        encode_double_quoted_attribute(&return_to),
    ))
}

async fn timeline(
    BrowserOAuthSession(session): BrowserOAuthSession<PublicResolver, FileAuthStore>,
) -> Result<Html<String>, AppError> {
    let agent: Agent<_> = Agent::from(session);
    let response = agent
        .send(GetTimeline::new().limit(10).build())
        .await
        .map_err(AppError::from_display)?;
    let timeline = response.into_output().map_err(AppError::from_display)?;

    let mut html = String::from(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Timeline</title>
</head>
<body>
  <main>
    <h1>Timeline</h1>
    <p><a href="/api/session">View strict extractor session JSON</a></p>
    <ol>
"#,
    );
    for item in timeline.feed {
        html.push_str("      <li><strong>");
        html.push_str(&encode_text(item.post.author.handle.as_str()));
        html.push_str("</strong>: <code>");
        html.push_str(&encode_text(
            &serde_json::to_string(&item.post.record).unwrap_or_default(),
        ));
        html.push_str("</code></li>\n");
    }
    html.push_str(
        r#"    </ol>
    <form method="post" action="/oauth/logout">
      <button type="submit">Log out</button>
    </form>
  </main>
</body>
</html>"#,
    );
    Ok(Html(html))
}

async fn strict_session_json(
    ExtractOAuthSession(session): ExtractOAuthSession<PublicResolver, FileAuthStore>,
) -> Json<serde_json::Value> {
    let (did, session_id) = session.session_info().await;
    Json(serde_json::json!({ "did": did, "session_id": session_id }))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("JACQUARD_AXUM_OAUTH_LOG"))
        .init();

    let args = Args::parse();
    let paths = ExamplePaths::new(args.data_dir);
    paths.create_data_dir()?;

    let client_data = ClientData::new(
        Some(load_or_generate_keyset(&paths.keyset)?),
        hosted_client_metadata(&args.base_url)?,
    );

    let state = AppState {
        oauth: Arc::new(OAuthClient::new(
            FileAuthStore::new(paths.sessions.to_string_lossy().into_owned()),
            client_data,
            reqwest::Client::new(),
        )),
        oauth_config: OAuthWebConfig::default(),
        cookie_key: load_or_generate_cookie_key(&paths.cookie_key)?,
    };

    let app = Router::new()
        .route("/", get(|| async { Html(HOME_PAGE) }))
        .route("/oauth/login", get(login_page))
        .route("/timeline", get(timeline))
        .route("/api/session", get(strict_session_json))
        .merge(oauth_routes::<PublicResolver, FileAuthStore, AppState>())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .into_diagnostic()?;
    axum::serve(listener, app).await.into_diagnostic()?;
    Ok(())
}

const HOME_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Jacquard Axum OAuth example</title>
</head>
<body>
  <main>
    <h1>Jacquard Axum OAuth example</h1>
    <p>This is a hosted OAuth client example. Its OAuth client metadata is served
    from <code>/oauth-client-metadata.json</code> by the same Axum state that
    starts OAuth, handles callbacks, restores sessions, and logs out.</p>
    <p><a href="/timeline">Open the authenticated timeline</a></p>
  </main>
</body>
</html>"#;

fn hosted_client_metadata(base_url: &str) -> Result<AtprotoClientMetadata<SmolStr>> {
    let base_url = base_url.trim_end_matches('/');
    let client_uri = Uri::parse(base_url.to_owned())
        .map_err(|(err, _)| miette!("invalid --base-url `{}`: {err}", base_url))?;
    if client_uri.scheme().as_str() != "https" {
        bail!("--base-url must be an externally reachable https:// origin");
    }
    if client_uri.path().as_str() != "/"
        || client_uri.query().is_some()
        || client_uri.fragment().is_some()
    {
        bail!("--base-url must be an origin only, such as https://example.com");
    }

    let client_id = Uri::parse(format!("{base_url}/oauth-client-metadata.json"))
        .map_err(|(err, _)| miette!("invalid client metadata URL: {err}"))?;
    let redirect_uri = Uri::parse(format!("{base_url}/oauth/callback"))
        .map_err(|(err, _)| miette!("invalid OAuth callback URL: {err}"))?;
    let scopes = Scopes::new(SmolStr::new_static("atproto rpc:*"))
        .map_err(|err| miette!("invalid OAuth scopes: {err}"))?;

    Ok(AtprotoClientMetadata {
        client_id,
        client_uri: Some(client_uri),
        redirect_uris: vec![redirect_uri],
        grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        scopes,
        jwks_uri: None,
        client_name: Some(SmolStr::new_static("Jacquard Axum OAuth example")),
        logo_uri: None,
        tos_uri: None,
        privacy_policy_uri: None,
    })
}

#[derive(Debug)]
struct ExamplePaths {
    data_dir: PathBuf,
    cookie_key: PathBuf,
    keyset: PathBuf,
    sessions: PathBuf,
}

impl ExamplePaths {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            cookie_key: data_dir.join("private-cookie.key"),
            keyset: data_dir.join("oauth-client-keyset.json"),
            sessions: data_dir.join("oauth-sessions.json"),
            data_dir,
        }
    }

    fn create_data_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.data_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to create {}", self.data_dir.display()))
    }
}

fn load_or_generate_cookie_key(path: &PathBuf) -> Result<Key> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() == 64 => Ok(Key::from(&bytes)),
        Ok(_) => bail!("{} must contain exactly 64 bytes", path.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let mut bytes = [0_u8; 64];
            OsRng.fill_bytes(&mut bytes);
            write_secret(path, &bytes)?;
            Ok(Key::from(&bytes))
        }
        Err(err) => Err(err)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read {}", path.display())),
    }
}

fn load_or_generate_keyset(path: &PathBuf) -> Result<Keyset> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to parse {}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let keyset = Keyset::generate_es256("jacquard-axum-example")
                .map_err(|err| miette!("failed to generate OAuth client keyset: {err}"))?;
            let bytes = serde_json::to_vec_pretty(&keyset).into_diagnostic()?;
            write_secret(path, &bytes)?;
            Ok(keyset)
        }
        Err(err) => Err(err)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read {}", path.display())),
    }
}

fn write_secret(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to restrict permissions on {}", path.display()))?;
    }
    Ok(())
}

#[derive(Debug)]
struct AppError(String);

impl AppError {
    fn from_display(err: impl std::fmt::Display) -> Self {
        Self(err.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "InternalServerError", "message": self.0 })),
        )
            .into_response()
    }
}
