//! Session bootstrap and scenario support shared by e2e targets.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jacquard::client::credential_session::CredentialSession;
use jacquard::client::{Agent, AtpSession};
use jacquard_common::deps::fluent_uri;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::http_client::HttpClient;
use jacquard_common::session::{MemorySessionStore, SessionKey};
use jacquard_common::xrpc::{Response, XrpcExt, XrpcRequest};
use jacquard_common::{AuthorizationToken, DefaultStr};
use jacquard_identity::JacquardResolver;
use jacquard_identity::resolver::{DidStep, HandleStep, ResolverOptions};

use crate::provider::{FixtureIdentity, Provider, ProviderContext};
use crate::transport::{AllowedHost, FixtureTransport, TransportAllowlist};

/// Logical fixture hostnames served by the native gateway ingress.
///
/// Tranquil's did:web hosts are loopback (`127.0.0.1%3A<port>`) and resolve
/// without DNS; they're excluded. Handle hosts serve
/// `/.well-known/atproto-did`; the PDS host terminates TLS for the
/// advertised service endpoint.
pub fn fixture_hostnames(provider: Provider) -> Vec<String> {
    let mut hosts = vec![
        provider.primary_identity().handle.to_string(),
        provider.member_identity().handle.to_string(),
        format!("pds.{}.jacquard-e2e.test", provider.name()),
        // OAuth client host: not a special-use TLD (see oauth::CLIENT_ID).
        "client.jacquard-e2e.dev".to_string(),
        "service.jacquard-e2e.dev".to_string(),
    ];
    // did:web hosts for both providers: reference identity hosts and the
    // Tranquil localhost-exception host (its member DID lives under a path,
    // so only the host segment appears here).
    let identity_host = |did: &str| {
        did.strip_prefix("did:web:")
            .expect("fixture DIDs are did:web")
            .split(':')
            .next()
            .expect("non-empty host")
            .to_string()
    };
    hosts.push(identity_host(provider.primary_identity().did));
    hosts.push(identity_host(provider.member_identity().did));
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Send one typed XRPC request through the allowlisted fixture transport.
/// Errors carry scenario context without leaking tokens; callers decode the
/// typed response (owned or borrowed) from the returned wrapper. XRPC 4xx
/// bodies arrive as `Ok(Response)` — decode via `into_output()`/`parse()` to
/// surface typed errors.
pub(crate) async fn send_xrpc<R>(
    stack: &FixtureStack,
    request: &R,
    bearer: Option<&str>,
    endpoint: &str,
) -> Result<Response<R::Response>, String>
where
    R: XrpcRequest + serde::Serialize + Sync,
    R::Response: Send + Sync,
{
    let base = fluent_uri::Uri::parse(stack.context.coordinates.provider_url.as_str())
        .map_err(|e| format!("parse provider URL: {e}"))?;

    let mut call = stack.transport.xrpc(base);
    if let Some(token) = bearer {
        call = call.auth(AuthorizationToken::Bearer(DefaultStr::new(token)));
    }
    call.send(request)
        .await
        .map_err(|e| stack.sanitize_error(endpoint, e))
}

/// Write a DID document to disk (durable, so it survives ingress restarts)
/// and to the running ingress's in-memory vhost map via the admin endpoint
/// (always HTTPS through the socat passthrough).
pub(crate) async fn write_doc_durably(
    stack: &FixtureStack,
    host: &str,
    doc: &serde_json::Value,
) -> Result<(), String> {
    let dir = stack.files.root.join("identities");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create identities dir: {e}"))?;
    std::fs::write(
        dir.join(format!("{host}.did.json")),
        serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write DID document: {e}"))?;

    let url = format!("https://{host}/_admin/did/{host}");
    let request = http::Request::builder()
        .method(http::Method::PUT)
        .uri(url)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(doc).expect("serialize did doc"))
        .map_err(|e| e.to_string())?;
    let response = HttpClient::send_http(stack.transport.as_ref(), request)
        .await
        .map_err(|e| stack.sanitize_error("ingress did publish", e))?;
    if response.status().as_u16() != 204 {
        return Err(format!(
            "ingress rejected DID document update: HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok(())
}

/// Fixture data file paths under the run's fixture root.
pub struct FixtureFiles {
    pub root: PathBuf,
}

impl FixtureFiles {
    /// The per-run CA certificate (PEM) trusted by the native transport.
    pub fn ca_pem(&self) -> PathBuf {
        self.root.join("e2e-ca.pem")
    }

    /// The provider's bootstrap app password. Read from fixture data, never
    /// from the environment.
    pub fn app_password(&self, provider: Provider) -> PathBuf {
        self.root.join(provider.name()).join("app-password")
    }

    /// The second (member) fixture identity's app password, used by the
    /// reference provider's spaces membership-boundary scenarios.
    pub fn member_app_password(&self, provider: Provider) -> PathBuf {
        self.root.join(provider.name()).join("member-app-password")
    }

    /// A deterministic fixture identity's DID document JSON.
    pub fn did_document(&self, identity: &FixtureIdentity) -> PathBuf {
        self.root.join("identities").join(format!(
            "{}.did.json",
            identity.did.trim_start_matches("did:web:")
        ))
    }
}

/// The assembled native fixture stack for one provider run.
pub struct FixtureStack {
    pub context: ProviderContext,
    pub transport: Arc<FixtureTransport>,
    pub files: FixtureFiles,
}

impl FixtureStack {
    /// Coordinate one bootstrap across nextest worker processes.
    pub async fn ensure_bootstrapped(&self) -> Result<(), String> {
        let provider_dir = self.files.root.join(self.context.provider.name());
        let lock = provider_dir.join("bootstrap.lock");
        let marker = provider_dir.join("bootstrapped");
        if marker.exists() {
            return Ok(());
        }

        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(_) => {
                let result = match self.context.provider {
                    Provider::Reference => {
                        crate::reference_bootstrap::ensure_reference_account(self).await
                    }
                    Provider::Tranquil => {
                        crate::reference_bootstrap::ensure_tranquil_account(self).await
                    }
                };
                let _ = std::fs::remove_file(&lock);
                result
            }
            Err(_) => {
                for _ in 0..60 {
                    if marker.exists() {
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Err(format!(
                    "timed out waiting for sibling process to finish bootstrap \
                     (stale lock at {}?)",
                    lock.display()
                ))
            }
        }
    }

    /// Build the fixture stack from lifecycle environment variables.
    pub fn from_env() -> Result<Self, String> {
        Self::from_context(ProviderContext::from_env()?)
    }

    /// Build the fixture stack from lifecycle coordinates: a reqwest client
    /// with the per-run CA and per-host resolve mappings, wrapped in the
    /// allowlist-enforcing transport.
    pub fn from_context(context: ProviderContext) -> Result<Self, String> {
        let coords = &context.coordinates;
        let files = FixtureFiles {
            root: PathBuf::from(&coords.fixture_root),
        };

        let ca_pem = std::fs::read(&files.ca_pem())
            .map_err(|e| format!("read CA {}: {e}", files.ca_pem().display()))?;
        let certificate = reqwest::Certificate::from_pem(&ca_pem)
            .map_err(|e| format!("parse CA certificate: {e}"))?;

        // Fixture hostnames resolve to the socat passthrough's bridge
        // address, which owns 443 and forwards to the native ingress — the
        // same path containers use. did:web URLs carry no explicit port, so
        // the allowlist entry must be port 443.
        let proxy_ip = std::env::var("JACQUARD_E2E_PROXY_IP").map_err(|_| {
            "missing required env var JACQUARD_E2E_PROXY_IP (exported by the lifecycle controller)"
        })?;
        let mut builder = reqwest::Client::builder()
            .add_root_certificate(certificate)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy();
        for host in fixture_hostnames(context.provider) {
            builder = builder.resolve(
                &host,
                format!("{proxy_ip}:443")
                    .parse::<std::net::SocketAddr>()
                    .map_err(|e| e.to_string())?,
            );
        }
        let client = builder
            .build()
            .map_err(|e| format!("build fixture client: {e}"))?;

        // The provider PDS is addressed by its container IP over plain HTTP
        // on Docker's bridge; identity and app hosts stay HTTPS-only.
        let provider_url =
            reqwest::Url::parse(&coords.provider_url).map_err(|e| format!("provider URL: {e}"))?;
        let provider_entry = AllowedHost {
            host: provider_url
                .host_str()
                .ok_or("provider URL has no host")?
                .to_string(),
            port: provider_url
                .port_or_known_default()
                .ok_or("provider URL has no port")?,
            scheme: "http",
        };
        let allowlist = TransportAllowlist::new(
            fixture_hostnames(context.provider)
                .into_iter()
                .map(|host| AllowedHost {
                    host,
                    port: 443,
                    scheme: "https",
                })
                .chain([provider_entry]),
        );
        let transport = Arc::new(FixtureTransport::new(client, allowlist));

        Ok(Self {
            context,
            transport,
            files,
        })
    }

    /// A resolver restricted to the fixture: `did:web` over HTTPS
    /// only, no handle steps, no PDS fallback, no public handle fallback, and
    /// no PLC step that could ever be consulted.
    pub fn resolver(&self) -> JacquardResolver<FixtureTransport> {
        let opts = ResolverOptions {
            did_order: vec![DidStep::DidWebHttps],
            handle_order: vec![HandleStep::HttpsWellKnown],
            pds_fallback: None,
            public_fallback_for_handle: false,
            ..ResolverOptions::default()
        };
        // The resolver shares the allowlisted transport, so identity
        // resolution cannot leave the fixture network either.
        JacquardResolver::new((*self.transport).clone(), opts)
    }

    /// Authenticate the provider's primary fixture identity through
    /// Jacquard's credential-session API.
    pub async fn credential_agent(
        &self,
    ) -> Result<
        Agent<
            CredentialSession<
                MemorySessionStore<SessionKey, AtpSession>,
                JacquardResolver<FixtureTransport>,
            >,
        >,
        String,
    > {
        self.credential_agent_for(
            &self.context.identity,
            self.files.app_password(self.context.provider),
        )
        .await
    }

    /// Authenticate an arbitrary fixture identity (primary or member) through
    /// Jacquard's credential-session API, using the app password the
    /// bootstrap left at `password_file`.
    pub async fn credential_agent_for(
        &self,
        identity: &crate::provider::FixtureIdentity,
        password_file: PathBuf,
    ) -> Result<
        Agent<
            CredentialSession<
                MemorySessionStore<SessionKey, AtpSession>,
                JacquardResolver<FixtureTransport>,
            >,
        >,
        String,
    > {
        let password = std::fs::read_to_string(&password_file)
            .map_err(|e| format!("read app password {}: {e}", password_file.display()))?
            .trim()
            .to_string();

        let pds = Uri::parse(self.context.coordinates.provider_url.as_str())
            .map_err(|e| format!("parse provider URL: {e}"))?
            .to_owned();

        let store = Arc::new(MemorySessionStore::default());
        let session = CredentialSession::new(store, Arc::new(self.resolver()));
        session.set_endpoint(pds.clone()).await;
        session
            .login(
                identity.handle,
                &password,
                None,
                None,
                None,
                // The fixture PDS terminates no TLS: its DID-doc
                // serviceEndpoint (https://) is container-internal. The host
                // driver passes the direct bridge address, as a real client
                // would pass its known PDS/entryway URL.
                Some(pds),
            )
            .await
            .map_err(|e| self.sanitize_error("createSession", e))?;

        Ok(Agent::new(session))
    }

    /// A cookie-aware HTTP client for driving the provider's authorization
    /// UI: same CA, per-host resolve mappings, redirect and proxy behaviour
    /// as the fixture transport, plus a cookie store.
    pub fn cookie_client(&self) -> Result<reqwest::Client, String> {
        let coords = &self.context.coordinates;
        let ca_pem = std::fs::read(self.files.ca_pem())
            .map_err(|e| format!("read CA {}: {e}", self.files.ca_pem().display()))?;
        let certificate = reqwest::Certificate::from_pem(&ca_pem)
            .map_err(|e| format!("parse CA certificate: {e}"))?;
        let proxy_ip = std::env::var("JACQUARD_E2E_PROXY_IP")
            .map_err(|_| "missing required env var JACQUARD_E2E_PROXY_IP".to_string())?;
        let mut builder = reqwest::Client::builder()
            .add_root_certificate(certificate)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .cookie_store(true);
        for host in fixture_hostnames(self.context.provider) {
            builder = builder.resolve(
                &host,
                format!("{proxy_ip}:443")
                    .parse::<std::net::SocketAddr>()
                    .map_err(|e| e.to_string())?,
            );
        }
        let _ = coords;
        builder
            .build()
            .map_err(|e| format!("build cookie client: {e}"))
    }

    pub async fn direct_service_request(
        &self,
        token: &str,
        uri: &str,
    ) -> Result<http::Response<Vec<u8>>, String> {
        let request = http::Request::get(uri)
            .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Vec::new())
            .map_err(|e| e.to_string())?;
        HttpClient::send_http(self.transport.as_ref(), request)
            .await
            .map_err(|e| self.sanitize_error("service direct request", e))
    }

    /// Wrap an error with scenario context while scrubbing credential-shaped
    /// material. Tokens, passwords, and authorization headers never appear in
    /// diagnostics; the body excerpt keeps only the first 200 bytes of
    /// printable text.
    pub fn sanitize_error<E: std::fmt::Display>(&self, endpoint: &str, error: E) -> String {
        let text = error.to_string();
        let excerpt: String = text.chars().filter(|c| !c.is_control()).take(400).collect();
        format!(
            "provider {} endpoint {} (auth=credential, digest {}): {excerpt} \
             [diagnostics: {}/]",
            self.context.provider.name(),
            endpoint,
            self.context.coordinates.effective_digest,
            self.context.coordinates.artifact_dir,
        )
    }
}
