//! Native ingress for the e2e harness: the host-side HTTPS fixture bound to
//! the Docker bridge gateway used by the Compose services.
//!
//! Serves, per virtual host:
//! - `GET /.well-known/did.json` for every fixture identity (seeded from
//!   `<fixture-root>/identities/*.did.json`, updatable at runtime through
//!   `PUT /_admin/did/:host` so bootstrap flows can rotate the `#atproto`
//!   key exactly as the provider contracts require);
//! - `GET /client-metadata.json` (the OAuth client metadata document);
//! - `GET /e2e-health` returning a unique fixture marker;
//! - the generated `test.blobby.get` endpoint on the service host, guarded by
//!   Jacquard's strict service-auth extractor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, put};
use jacquard_axum::service_auth::{ExtractServiceAuth, ServiceAuthConfig, require_service_id};
use jacquard_axum::{ExtractXrpc, IntoRouter, XrpcResponse};
use jacquard_codegen_tests::pretty::test_blobby::get::{GetOutput, GetRequest};
use jacquard_codegen_tests::pretty::test_blobby::record::Record;
use jacquard_common::DefaultStr;
use jacquard_common::deps::bytes::Bytes;
use jacquard_common::types::string::Did;
use jacquard_e2e::provider::TRANQUIL_SERVICE_DID;
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

type DidDocs = Arc<Mutex<HashMap<String, String>>>;

#[derive(Clone)]
struct IngressResolver {
    docs: DidDocs,
    options: ResolverOptions,
}

impl IdentityResolver for IngressResolver {
    fn options(&self) -> &ResolverOptions {
        &self.options
    }

    fn resolve_handle<S: jacquard_common::BosStr + Sync>(
        &self,
        _handle: &jacquard_common::types::string::Handle<S>,
    ) -> impl std::future::Future<Output = Result<Did, IdentityError>> + Send {
        async { Err(IdentityError::handle_resolution_exhausted()) }
    }

    fn resolve_did_doc<S: jacquard_common::BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> impl std::future::Future<Output = Result<DidDocResponse, IdentityError>> + Send {
        let docs = self.docs.clone();
        let did = did.as_str().to_string();
        async move {
            let key = did
                .strip_prefix("did:web:")
                .ok_or_else(|| IdentityError::unsupported_did_method(&did))?;
            let key = key.replace("%3A", ":").replace(":service", "/service");
            let body = docs
                .lock()
                .expect("did doc map poisoned")
                .get(&key)
                .cloned()
                .ok_or_else(IdentityError::handle_resolution_exhausted)?;
            Ok(DidDocResponse {
                buffer: Bytes::from(body),
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                requested: Some(Did::new_owned(&did).expect("validated DID")),
            })
        }
    }
}

type ServiceAuthState = ServiceAuthConfig<IngressResolver>;
type IngressState = (DidDocs, Arc<PathBuf>);

const SERVICE_DID: &str = TRANQUIL_SERVICE_DID;
const SERVICE_ID: &str = "jacquard_e2e";
const PNG_CID: &str = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("ingress: missing required env var {key}"))
}

async fn did_json(
    State((docs, _root)): State<IngressState>,
    uri: axum::http::Uri,
    headers: header::HeaderMap,
) -> impl IntoResponse {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.split(':').next().unwrap_or(h).to_string())
    else {
        return (StatusCode::BAD_REQUEST, "missing Host header").into_response();
    };
    let key = if uri.path() == "/.well-known/did.json" {
        host
    } else {
        format!(
            "{}{}",
            host,
            uri.path().strip_suffix("/did.json").unwrap_or(uri.path())
        )
    };
    let docs = docs.lock().expect("did doc map poisoned");
    let Some(doc) = docs.get(&key) else {
        return (StatusCode::NOT_FOUND, "unknown fixture host").into_response();
    };
    // Route every stored document through Jacquard's own `DidDocument` type
    // (the type `jacquard_axum::did_web::did_web_router` serves): documents
    // that fail to round-trip it are rejected rather than served.
    match serde_json::from_str::<jacquard_common::types::did_doc::DidDocument<DefaultStr>>(doc) {
        Ok(parsed) => (
            StatusCode::OK,
            // `application/did+ld+json` is accepted by every consumer: the
            // reference resolver's MIME check allows only
            // `application/(did+ld+)?json` — notably NOT the bare
            // `application/did+json` — while Tranquil accepts any JSON.
            [(header::CONTENT_TYPE, "application/did+ld+json")],
            serde_json::to_string(&parsed).expect("serialized DidDocument"),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("stored DID document failed DidDocument validation: {e}"),
        )
            .into_response(),
    }
}

async fn put_did(
    State((docs, _root)): State<IngressState>,
    Path(host): Path<String>,
    body: String,
) -> impl IntoResponse {
    // Only the bridge and the host can reach the gateway bind address, and
    // only the scenario driver and providers on the run's bridge ever talk
    // to this endpoint. Still, the body must be a JSON object.
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) if value.is_object() => {
            docs.lock()
                .expect("did doc map poisoned")
                .insert(host, body);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => (StatusCode::BAD_REQUEST, "body must be a JSON object").into_response(),
    }
}

async fn client_metadata(State((_docs, fixture_root)): State<IngressState>) -> impl IntoResponse {
    // The full AtprotoClientMetadata is produced by the scenario process via
    // Jacquard's own types and written to the fixture root; read per request
    // so bootstrap-written updates are visible without an ingress restart.
    match tokio::fs::read_to_string(fixture_root.join("client-metadata.json")).await {
        Ok(body) => ([(header::CONTENT_TYPE, "application/json")], body).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "client metadata not yet written by bootstrap",
        )
            .into_response(),
    }
}

async fn health() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"ok","fixture":"jacquard-e2e"}"#,
    )
}

/// `https://{handle}/.well-known/atproto-did` — the HTTPS handle-resolution
/// step Jacquard's resolver performs. Keyed by the virtual host (the handle's
/// domain), seeded from `<fixture-root>/handles/<host>`.
async fn atproto_did(headers: header::HeaderMap) -> impl IntoResponse {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.split(':').next().unwrap_or(h).to_string())
    else {
        return (StatusCode::BAD_REQUEST, "missing Host header").into_response();
    };
    let Ok(root) = std::env::var("INGRESS_FIXTURE_ROOT") else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "ingress misconfigured").into_response();
    };
    match std::fs::read_to_string(std::path::Path::new(&root).join("handles").join(&host)) {
        Ok(did) => (
            [(header::CONTENT_TYPE, "text/plain")],
            did.trim().to_string(),
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "unknown handle host").into_response(),
    }
}

/// Reverse-proxy requests whose Host is `pds.<provider>.jacquard-e2e.test`
/// to the provider PDS's plain HTTP port. The ingress owns the TLS
/// termination for the advertised `https://pds...` service endpoint, so
/// clients (and the PDS's own DID-doc validation) see a genuinely reachable
/// HTTPS origin. Upstream address comes from `INGRESS_PDS_UPSTREAM`
/// (`host:port`), set by the lifecycle controller after IP discovery.
async fn pds_proxy(
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: header::HeaderMap,
    State((_, fixture_root)): State<IngressState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // The upstream address is discovered after this process starts (the PDS
    // container must exist first), so it arrives through a file written by
    // the lifecycle controller rather than an environment variable. Reading
    // per request avoids an ingress restart, which would break providers'
    // keep-alive connections mid-run.
    let upstream = match tokio::fs::read_to_string(fixture_root.join("pds-upstream")).await {
        Ok(s) => s.trim().to_string(),
        Err(_) => return (StatusCode::NOT_FOUND, "no PDS upstream configured").into_response(),
    };
    if upstream.is_empty() {
        return (StatusCode::NOT_FOUND, "no PDS upstream configured").into_response();
    }
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.split(':').next().unwrap_or(h).to_string())
    else {
        return (StatusCode::BAD_REQUEST, "missing Host header").into_response();
    };
    if !host.starts_with("pds.") {
        return (StatusCode::NOT_FOUND, "unknown fixture host").into_response();
    }

    let path_and_query = match uri.query() {
        Some(q) => format!("{}?{}", uri.path(), q),
        None => uri.path().to_string(),
    };
    let url = format!("http://{upstream}{path_and_query}");
    // Redirects are never followed: the proxied response must reach the
    // original caller untouched. Following one would re-enter this ingress
    // over TLS (fixture CA untrusted by reqwest) and fail the whole hop.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("proxy client");
    let mut request = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        url,
    );
    eprintln!("ingress: pds_proxy -> {method} {path_and_query} upstream={upstream}");
    for (name, value) in &headers {
        if name == header::HOST {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            request = request.header(name, value);
        }
    }
    match request.body(body).send().await {
        Ok(response) => {
            let status = response.status();
            let mut builder = axum::http::Response::builder().status(status);
            for (name, value) in response.headers() {
                // Hop-by-hop and length headers belong to the proxy
                // connection, not the forwarded response; hyper re-derives
                // them. Forwarding `transfer-encoding` poisons the builder.
                if matches!(
                    name.as_str(),
                    "connection"
                        | "keep-alive"
                        | "transfer-encoding"
                        | "content-length"
                        | "upgrade"
                ) {
                    continue;
                }
                builder = builder.header(name, value);
            }
            match response.bytes().await {
                Ok(bytes) => builder
                    .body(axum::body::Body::from(bytes))
                    .unwrap_or_else(|e| (StatusCode::BAD_GATEWAY, e.to_string()).into_response()),
                Err(e) => {
                    eprintln!("ingress: pds_proxy upstream body error: {e}");
                    (StatusCode::BAD_GATEWAY, e.to_string()).into_response()
                }
            }
        }
        Err(e) => {
            eprintln!("ingress: pds_proxy upstream send error: {e:?}");
            (StatusCode::BAD_GATEWAY, e.to_string()).into_response()
        }
    }
}

fn seed_documents(fixture_root: &PathBuf, docs: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(fixture_root.join("identities")) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // "<host>.did.json" serves "<host>".
        let host = stem.strip_suffix(".did").unwrap_or(stem);
        if let Ok(body) = std::fs::read_to_string(&path) {
            docs.insert(host.to_string(), body);
        }
    }
}

fn service_record() -> Record {
    serde_json::from_value(serde_json::json!({
        "primary": {
            "$type": "blob",
            "ref": { "$link": PNG_CID },
            "mimeType": "image/png",
            "size": 32
        },
        "note": "service proxy"
    }))
    .expect("service fixture record is valid")
}

async fn service_get(
    ExtractServiceAuth(auth): ExtractServiceAuth,
    ExtractXrpc(request): ExtractXrpc<GetRequest>,
) -> XrpcResponse<GetRequest> {
    let mut extra_data = std::collections::BTreeMap::new();
    extra_data.insert("serviceDid".into(), auth.did().as_str().into());
    extra_data.insert("audience".into(), auth.aud().as_str().into());
    if let Some(service) = auth.service() {
        extra_data.insert("service".into(), service.into());
    }
    if let Some(lxm) = auth.lxm() {
        extra_data.insert("lxm".into(), lxm.as_str().into());
    }
    if let Some(jti) = auth.jti() {
        extra_data.insert("jti".into(), jti.into());
    }
    XrpcResponse(GetOutput {
        uri: request.uri,
        value: service_record(),
        extra_data: Some(extra_data),
    })
}

fn load_certs(pem: &[u8]) -> Vec<CertificateDer<'static>> {
    let mut certs = Vec::new();
    parse_pem_blocks(pem, "CERTIFICATE", |der| {
        certs.push(CertificateDer::from(der));
    });
    certs
}

fn load_key(pem: &[u8]) -> PrivateKeyDer<'static> {
    let mut keys = Vec::new();
    parse_pem_blocks(pem, "PRIVATE KEY", |der| {
        keys.push(PrivateKeyDer::Pkcs8(der.into()));
    });
    keys.into_iter()
        .next()
        .expect("ingress key PEM contains a private key")
}

// Minimal PEM parser: sufficient for the two files this harness itself
// generated. Does not handle arbitrary PEM catalogues.
fn parse_pem_blocks(pem: &[u8], label: &str, mut f: impl FnMut(Vec<u8>)) {
    use base64::Engine;
    let text = String::from_utf8_lossy(pem);
    let mut current: Option<(String, String)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("-----BEGIN ") {
            let name = name.strip_suffix("-----").expect("closed BEGIN");
            current = Some((name.to_string(), String::new()));
        } else if line.starts_with("-----END") {
            if let Some((name, body)) = current.take() {
                if name.contains(label) {
                    if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(&body) {
                        f(der);
                    }
                }
            }
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
        }
    }
}

fn service_auth_config(docs: DidDocs) -> ServiceAuthState {
    ServiceAuthConfig::new(
        Did::new_static(SERVICE_DID).expect("service DID"),
        IngressResolver {
            docs,
            options: ResolverOptions::default(),
        },
    )
}

#[tokio::main]
async fn main() {
    let bind_addr = env("INGRESS_BIND");
    let port: u16 = env("INGRESS_PORT").parse().expect("INGRESS_PORT is a port");
    let fixture_root = PathBuf::from(env("INGRESS_FIXTURE_ROOT"));

    let docs: DidDocs = Arc::new(Mutex::new(HashMap::new()));
    seed_documents(
        &fixture_root,
        &mut docs.lock().expect("did doc map poisoned"),
    );
    let service_doc = serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/multikey/v1"
        ],
        "id": SERVICE_DID,
        "service": [{
            "id": format!("#{SERVICE_ID}"),
            "type": "AtprotoService",
            "serviceEndpoint": "https://service.jacquard-e2e.dev"
        }]
    });
    docs.lock().expect("did doc map poisoned").insert(
        "localhost.jacquard-e2e.test/service".to_string(),
        serde_json::to_string(&service_doc).expect("service DID document"),
    );
    let fixture_root = std::sync::Arc::new(fixture_root);
    let service_auth = service_auth_config(docs.clone());

    let service_router = GetRequest::into_router::<_, ServiceAuthState, _>(service_get)
        .route_layer(require_service_id(SERVICE_ID));
    let app = Router::new()
        .route("/.well-known/did.json", get(did_json))
        .route("/service/did.json", get(did_json))
        .route("/.well-known/atproto-did", get(atproto_did))
        .route("/client-metadata.json", get(client_metadata))
        .route("/e2e-health", get(health))
        .route("/_admin/did/{host}", put(put_did))
        .merge(service_router.with_state(service_auth))
        .route("/", get(pds_proxy))
        .fallback(pds_proxy)
        .with_state((docs, fixture_root.clone()));

    // Optional plain-HTTP listener for Tranquil's test-only localhost DID
    // fetch path. The listener is bound to the Docker bridge gateway rather
    // than a published host interface.
    if let Ok(http_port) = std::env::var("INGRESS_HTTP_PORT") {
        let http_bind = std::env::var("INGRESS_HTTP_BIND").unwrap_or_else(|_| env("INGRESS_BIND"));
        let http_listener = tokio::net::TcpListener::bind(format!("{http_bind}:{http_port}"))
            .await
            .expect("bind ingress HTTP listener");
        let app_http = app.clone();
        tokio::spawn(async move {
            axum::serve(http_listener, app_http)
                .await
                .expect("ingress HTTP server");
        });
        eprintln!("ingress: http listener on http://{http_bind}:{http_port}");
    }

    let certs =
        load_certs(&std::fs::read(fixture_root.join("ingress.pem")).expect("read ingress cert"));
    let key = load_key(&std::fs::read(fixture_root.join("ingress.key")).expect("read ingress key"));

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("valid ingress TLS configuration");
    let acceptor = TlsAcceptor::from(Arc::new(config));

    let listener = tokio::net::TcpListener::bind(format!("{bind_addr}:{port}"))
        .await
        .expect("bind ingress to the bridge gateway");

    eprintln!("ingress: listening on https://{bind_addr}:{port}");
    // Accept and serve each connection in its own task. Handshakes must not
    // serialize behind in-flight requests: the PDS's OAuth handlers open a
    // second TLS connection back to this ingress (client-metadata fetch)
    // while an earlier proxied connection is still being served, so a
    // sequential accept+handshake loop deadlocks both sides.
    loop {
        let accepted = listener.accept().await;
        match accepted {
            Ok((stream, _addr)) => {
                let acceptor = acceptor.clone();
                let app = app.clone();
                tokio::spawn(async move {
                    match acceptor.accept(stream).await {
                        Ok(tls) => {
                            let listener = OneConnectionListener(Some(tls));
                            if let Err(e) = axum::serve(listener, app).await {
                                eprintln!("ingress: connection serve error: {e}");
                            }
                        }
                        // A failed handshake terminates one connection, not
                        // the listener.
                        Err(_) => {}
                    }
                });
            }
            // Transient accept errors are skipped like failed handshakes.
            Err(_) => continue,
        }
    }
}

/// A `Listener` yielding exactly one pre-established TLS stream, for serving
/// a single already-handshaken connection through `axum::serve`.
struct OneConnectionListener(Option<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>);

impl axum::serve::Listener for OneConnectionListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.0.take() {
            Some(tls) => {
                let addr = tls
                    .get_ref()
                    .0
                    .peer_addr()
                    .unwrap_or_else(|_| "0.0.0.0:0".parse().expect("wildcard addr"));
                (tls, addr)
            }
            // `axum::serve` polls `accept` again after the connection ends
            // (to drive graceful shutdown); park forever instead of
            // completing, which would be interpreted as a new connection.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        match &self.0 {
            Some(tls) => tls.get_ref().0.local_addr(),
            None => Err(std::io::Error::other("listener exhausted")),
        }
    }
}
