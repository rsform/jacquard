use std::collections::VecDeque;
use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderValue, Method, Response as HttpResponse, StatusCode};
use jacquard::BosStr;
use jacquard::client::Agent;
use jacquard::deps::fluent_uri::Uri;
use jacquard::types::did::Did;
use jacquard::xrpc::XrpcClient;
use jacquard_common::http_client::HttpClient;
use jacquard_oauth::atproto::AtprotoClientMetadata;
use jacquard_oauth::client::OAuthSession;
use jacquard_oauth::resolver::OAuthResolver;
use jacquard_oauth::scopes::Scopes;
use jacquard_oauth::session::SessionRegistry;
use jacquard_oauth::session::{ClientData, ClientSessionData, DpopClientData};
use jacquard_oauth::types::{OAuthAuthorizationServerMetadata, OAuthTokenType, TokenSet};
use smol_str::{SmolStr, format_smolstr};
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct MockClient {
    queue: Arc<Mutex<VecDeque<http::Response<Vec<u8>>>>>,
    log: Arc<Mutex<Vec<http::Request<Vec<u8>>>>>,
}

impl MockClient {
    async fn push(&self, resp: http::Response<Vec<u8>>) {
        self.queue.lock().await.push_back(resp);
    }
}

impl HttpClient for MockClient {
    type Error = std::convert::Infallible;
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl core::future::Future<
        Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>,
    > + Send {
        let log = self.log.clone();
        let queue = self.queue.clone();
        async move {
            log.lock().await.push(request);
            Ok(queue.lock().await.pop_front().expect("no queued response"))
        }
    }
}
impl jacquard::identity::resolver::IdentityResolver for MockClient {
    fn options(&self) -> &jacquard::identity::resolver::ResolverOptions {
        use std::sync::LazyLock;
        static OPTS: LazyLock<jacquard::identity::resolver::ResolverOptions> =
            LazyLock::new(jacquard::identity::resolver::ResolverOptions::default);
        &OPTS
    }
    async fn resolve_handle<S: BosStr>(
        &self,
        _handle: &jacquard::types::string::Handle<S>,
    ) -> std::result::Result<Did, jacquard::identity::resolver::IdentityError> {
        Ok(Did::new_static("did:plc:alice").unwrap())
    }
    async fn resolve_did_doc<S: BosStr>(
        &self,
        _did: &Did<S>,
    ) -> std::result::Result<
        jacquard::identity::resolver::DidDocResponse,
        jacquard::identity::resolver::IdentityError,
    > {
        let doc = serde_json::json!({
            "@context": ["https://www.w3.org/ns/did/v1"],
            "id": "did:plc:alice",
            "alsoKnownAs": ["at://alice.bsky.social"],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": "https://pds.example.com"
            }]
        });
        Ok(jacquard::identity::resolver::DidDocResponse {
            buffer: Bytes::from(serde_json::to_vec(&doc).unwrap()),
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            requested: None,
        })
    }
}

impl OAuthResolver for MockClient {
    async fn get_authorization_server_metadata(
        &self,
        issuer: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, jacquard_oauth::resolver::ResolverError> {
        // Return minimal metadata with supported auth method "none" and DPoP support
        let mut md = OAuthAuthorizationServerMetadata::default();
        md.issuer = SmolStr::from(issuer);
        md.token_endpoint = format_smolstr!("{}/token", issuer);
        md.authorization_endpoint = format_smolstr!("{}/authorize", issuer);
        md.require_pushed_authorization_requests = Some(true);
        md.pushed_authorization_request_endpoint = Some(format_smolstr!("{}/par", issuer));
        md.token_endpoint_auth_methods_supported = Some(vec![SmolStr::from("none")]);
        md.dpop_signing_alg_values_supported = Some(vec![SmolStr::from("ES256")]);
        Ok(md)
    }

    async fn get_resource_server_metadata(
        &self,
        _pds: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, jacquard_oauth::resolver::ResolverError> {
        // Return metadata pointing to the same issuer as above.
        let mut md = OAuthAuthorizationServerMetadata::default();
        md.issuer = SmolStr::from("https://issuer");
        md.token_endpoint = SmolStr::from("https://issuer/token");
        md.authorization_endpoint = SmolStr::from("https://issuer/authorize");
        md.require_pushed_authorization_requests = Some(true);
        md.pushed_authorization_request_endpoint = Some(SmolStr::from("https://issuer/par"));
        md.token_endpoint_auth_methods_supported = Some(vec![SmolStr::from("none")]);
        md.dpop_signing_alg_values_supported = Some(vec![SmolStr::from("ES256")]);
        Ok(md)
    }

    async fn verify_issuer<S: BosStr + Sync>(
        &self,
        _server_metadata: &OAuthAuthorizationServerMetadata,
        _sub: &Did<S>,
    ) -> Result<jacquard::deps::fluent_uri::Uri<String>, jacquard_oauth::resolver::ResolverError>
    {
        Ok(
            jacquard::deps::fluent_uri::Uri::parse("https://pds.example.com")
                .unwrap()
                .to_owned(),
        )
    }
}

fn get_session_unauthorized() -> http::Response<Vec<u8>> {
    HttpResponse::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("DPoP realm=\"pds\", error=\"invalid_token\""),
        )
        .body(Vec::new())
        .unwrap()
}

fn get_session_unauthorized_body() -> http::Response<Vec<u8>> {
    HttpResponse::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(
            serde_json::to_vec(&serde_json::json!({
                "error":"InvalidToken"
            }))
            .unwrap(),
        )
        .unwrap()
}

fn token_use_dpop_nonce() -> http::Response<Vec<u8>> {
    HttpResponse::builder()
        .status(StatusCode::BAD_REQUEST)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("DPoP-Nonce", HeaderValue::from_static("n1"))
        .body(serde_json::to_vec(&serde_json::json!({"error":"use_dpop_nonce"})).unwrap())
        .unwrap()
}

fn token_refresh_ok() -> http::Response<Vec<u8>> {
    HttpResponse::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(
            serde_json::to_vec(&serde_json::json!({
                "access_token":"newacc",
                "token_type":"DPoP",
                "refresh_token":"newref",
                "expires_in": 3600
            }))
            .unwrap(),
        )
        .unwrap()
}

fn get_session_ok() -> http::Response<Vec<u8>> {
    HttpResponse::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(
            serde_json::to_vec(&serde_json::json!({
                "did":"did:plc:alice",
                "handle":"alice.bsky.social",
                "active":true
            }))
            .unwrap(),
        )
        .unwrap()
}

impl jacquard_oauth::dpop::DpopExt for MockClient {}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_xrpc_invalid_token_triggers_refresh_and_retries() {
    // (reopen test body since we inserted a trait impl)
    let client = Arc::new(MockClient::default());

    client.push(get_session_unauthorized()).await;
    client.push(token_use_dpop_nonce()).await;
    client.push(token_refresh_ok()).await;
    client.push(get_session_ok()).await;

    let mut path = std::env::temp_dir();
    path.push(format!("jacquard-oauth-test-{}.json", std::process::id()));
    std::fs::write(&path, "{}").unwrap();
    let store = jacquard::client::FileAuthStore::new(&path);

    let client_data = ClientData {
        keyset: None,
        config: AtprotoClientMetadata::new_localhost(
            None,
            Some(Scopes::new(SmolStr::new_static("atproto")).unwrap()),
        ),
    };
    use jacquard::IntoStatic;
    let session_data = ClientSessionData {
        account_did: Did::new_static("did:plc:alice").unwrap(),
        session_id: SmolStr::from("state"),
        host_url: Uri::parse("https://pds.example.com")
            .expect("valid uri")
            .to_owned(),
        authserver_url: SmolStr::new_static("https://issuer"),
        authserver_token_endpoint: SmolStr::from("https://issuer/token"),
        authserver_revocation_endpoint: None,
        scopes: Scopes::new(SmolStr::new_static("atproto")).unwrap(),
        dpop_data: DpopClientData {
            dpop_key: jacquard_oauth::utils::generate_key(&[SmolStr::from("ES256")]).unwrap(),
            dpop_authserver_nonce: SmolStr::from(""),
            dpop_host_nonce: SmolStr::from(""),
        },
        token_set: TokenSet {
            iss: SmolStr::from("https://issuer"),
            sub: Did::new_static("did:plc:alice").unwrap(),
            aud: SmolStr::from("https://pds.example.com"),
            scope: None,
            refresh_token: Some(SmolStr::from("rt1")),
            access_token: SmolStr::from("atk1"),
            token_type: OAuthTokenType::DPoP,
            expires_at: None,
        },
        resolved_scopes: None,
    }
    .into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    // Seed the store so refresh can load the session
    let data_store = ClientSessionData {
        account_did: Did::new_static("did:plc:alice").unwrap(),
        session_id: SmolStr::from("state"),
        host_url: Uri::parse("https://pds.example.com")
            .expect("valid uri")
            .to_owned(),
        authserver_url: SmolStr::new_static("https://issuer"),
        authserver_token_endpoint: SmolStr::from("https://issuer/token"),
        authserver_revocation_endpoint: None,
        scopes: Scopes::new(SmolStr::new_static("atproto")).unwrap(),
        dpop_data: DpopClientData {
            dpop_key: jacquard_oauth::utils::generate_key(&[SmolStr::from("ES256")]).unwrap(),
            dpop_authserver_nonce: SmolStr::from(""),
            dpop_host_nonce: SmolStr::from(""),
        },
        token_set: TokenSet {
            iss: SmolStr::from("https://issuer"),
            sub: Did::new_static("did:plc:alice").unwrap(),
            aud: SmolStr::from("https://pds.example.com"),
            scope: None,
            refresh_token: Some(SmolStr::from("rt1")),
            access_token: SmolStr::from("atk1"),
            token_type: OAuthTokenType::DPoP,
            expires_at: None,
        },
        resolved_scopes: None,
    }
    .into_static();
    registry.set(data_store).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .expect("xrpc send ok after auto-refresh");
    assert_eq!(resp.status(), StatusCode::OK);

    // Inspect the request log
    let log = client.log.lock().await;
    assert_eq!(log.len(), 4, "expected 4 HTTP calls");
    // 0: getSession with old token
    assert_eq!(log[0].method(), Method::GET);
    assert!(
        log[0]
            .headers()
            .get(http::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("DPoP ")
    );
    assert!(
        log[0]
            .uri()
            .to_string()
            .ends_with("/xrpc/com.atproto.server.getSession")
    );
    // 1 and 2: token refresh attempts
    assert_eq!(log[1].method(), Method::POST);
    assert!(log[1].uri().to_string().ends_with("/token"));
    assert!(log[1].headers().contains_key("DPoP"));
    assert_eq!(log[2].method(), Method::POST);
    assert!(log[2].uri().to_string().ends_with("/token"));
    assert!(log[2].headers().contains_key("DPoP"));
    // 3: retried getSession with new access token
    assert_eq!(log[3].method(), Method::GET);
    assert!(
        log[3]
            .headers()
            .get(http::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("DPoP newacc")
    );

    // Cleanup temp file
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_xrpc_invalid_token_body_triggers_refresh_and_retries() {
    let client = Arc::new(MockClient::default());

    // Queue responses: initial 401 with JSON body; token refresh 400(use_dpop_nonce); token refresh 200; retry getSession 200
    client.push(get_session_unauthorized_body()).await;
    client.push(token_use_dpop_nonce()).await;
    client.push(token_refresh_ok()).await;
    client.push(get_session_ok()).await;

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-oauth-test-body-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").unwrap();
    let store = jacquard::client::FileAuthStore::new(&path);

    let client_data = ClientData {
        keyset: None,
        config: AtprotoClientMetadata::new_localhost(
            None,
            Some(Scopes::new(SmolStr::new_static("atproto")).unwrap()),
        ),
    };
    use jacquard::IntoStatic;
    let session_data = ClientSessionData {
        account_did: Did::new_static("did:plc:alice").unwrap(),
        session_id: SmolStr::new_static("state"),
        host_url: Uri::parse("https://pds.example.com")
            .expect("valid uri")
            .to_owned(),
        authserver_url: SmolStr::new_static("https://issuer"),
        authserver_token_endpoint: SmolStr::from("https://issuer/token"),
        authserver_revocation_endpoint: None,
        scopes: Scopes::new(SmolStr::new_static("atproto")).unwrap(),
        dpop_data: DpopClientData {
            dpop_key: jacquard_oauth::utils::generate_key(&[SmolStr::from("ES256")]).unwrap(),
            dpop_authserver_nonce: SmolStr::from(""),
            dpop_host_nonce: SmolStr::from(""),
        },
        token_set: TokenSet {
            iss: SmolStr::from("https://issuer"),
            sub: Did::new_static("did:plc:alice").unwrap(),
            aud: SmolStr::from("https://pds.example.com"),
            scope: None,
            refresh_token: Some(SmolStr::from("rt1")),
            access_token: SmolStr::from("atk1"),
            token_type: OAuthTokenType::DPoP,
            expires_at: None,
        },
        resolved_scopes: None,
    }
    .into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .expect("xrpc send ok after auto-refresh");
    assert_eq!(resp.status(), StatusCode::OK);

    // Cleanup temp file
    let _ = std::fs::remove_file(&path);
}
