use std::{collections::VecDeque, future::Future, sync::Arc};

use axum::{
    Json, Router,
    extract::FromRef,
    http::{self, Response as HttpResponse, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use axum_extra::extract::cookie::Key;
use axum_test::TestServer;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use jacquard::{
    BosStr,
    common::{
        deps::{fluent_uri::Uri, smol_str::SmolStr},
        http_client::HttpClient,
        session::SessionKey,
        types::string::{Datetime, Did},
    },
    oauth::{
        atproto::{AtprotoClientMetadata, atproto_client_metadata},
        authstore::{ClientAuthStore, MemoryAuthStore},
        client::OAuthClient,
        resolver::OAuthResolver,
        scopes::Scopes,
        session::{ClientData, ClientSessionData, DpopClientData},
        types::{OAuthAuthorizationServerMetadata, OAuthTokenType, TokenSet},
    },
};
use jacquard_axum::oauth::{
    BrowserOAuthSession, ExtractOAuthSession, OAuthWebConfig, OAuthWebState, encode_session_key,
    routes, set_session_cookie,
};

#[derive(Clone, Default)]
struct MockClient {
    queue: Arc<tokio::sync::Mutex<VecDeque<http::Response<Vec<u8>>>>>,
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
        _request: http::Request<Vec<u8>>,
    ) -> impl core::future::Future<Output = Result<http::Response<Vec<u8>>, Self::Error>> + Send
    {
        let queue = self.queue.clone();
        async move { Ok(queue.lock().await.pop_front().expect("no queued response")) }
    }
}

impl jacquard::identity::resolver::IdentityResolver for MockClient {
    fn options(&self) -> &jacquard::identity::resolver::ResolverOptions {
        use std::sync::LazyLock;
        static OPTS: LazyLock<jacquard::identity::resolver::ResolverOptions> =
            LazyLock::new(jacquard::identity::resolver::ResolverOptions::default);
        &OPTS
    }

    async fn resolve_handle<S: BosStr + Sync>(
        &self,
        _handle: &jacquard::types::string::Handle<S>,
    ) -> Result<Did, jacquard::identity::resolver::IdentityError> {
        Ok(Did::new_static("did:plc:alice").unwrap())
    }

    async fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        _did: &jacquard::types::did::Did<S>,
    ) -> Result<
        jacquard::identity::resolver::DidDocResponse,
        jacquard::identity::resolver::IdentityError,
    > {
        let doc = alice_did_document_json();
        Ok(jacquard::identity::resolver::DidDocResponse {
            buffer: Bytes::from(serde_json::to_vec(&doc).unwrap()),
            status: StatusCode::OK,
            requested: None,
        })
    }
}

impl OAuthResolver for MockClient {
    async fn resolve_oauth(
        &self,
        _input: &str,
    ) -> Result<
        (
            OAuthAuthorizationServerMetadata,
            Option<jacquard::common::types::did_doc::DidDocument>,
        ),
        jacquard::oauth::resolver::ResolverError,
    > {
        let md = server_metadata("https://issuer");
        let did_doc = serde_json::from_value(alice_did_document_json()).unwrap();
        Ok((md, Some(did_doc)))
    }

    async fn get_authorization_server_metadata(
        &self,
        issuer: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, jacquard::oauth::resolver::ResolverError> {
        Ok(server_metadata(issuer))
    }

    async fn get_resource_server_metadata(
        &self,
        _pds: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, jacquard::oauth::resolver::ResolverError> {
        Ok(server_metadata("https://issuer"))
    }
}

impl jacquard::oauth::dpop::DpopExt for MockClient {}

fn alice_did_document_json() -> serde_json::Value {
    serde_json::json!({
        "@context": ["https://www.w3.org/ns/did/v1"],
        "id": "did:plc:alice",
        "alsoKnownAs": ["at://alice.bsky.social"],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": "https://pds.example.com"
        }]
    })
}

impl jacquard::identity::lexicon_resolver::LexiconSchemaResolver for MockClient {
    async fn resolve_lexicon_schema<S: BosStr + Sync>(
        &self,
        nsid: &jacquard::types::nsid::Nsid<S>,
    ) -> Result<
        jacquard::identity::lexicon_resolver::ResolvedLexiconSchema<'static>,
        jacquard::identity::lexicon_resolver::LexiconResolutionError,
    > {
        use jacquard::IntoStatic;
        Err(
            jacquard::identity::lexicon_resolver::LexiconResolutionError::new(
                jacquard::identity::lexicon_resolver::LexiconResolutionErrorKind::FetchFailed {
                    nsid: nsid.into_static().as_str().into(),
                },
                None,
            ),
        )
    }
}

#[derive(Clone)]
struct AppState {
    oauth: Arc<OAuthClient<MockClient, MemoryAuthStore>>,
    config: OAuthWebConfig,
    key: Key,
}

impl OAuthWebState<MockClient, MemoryAuthStore> for AppState {
    fn oauth_client(&self) -> &OAuthClient<MockClient, MemoryAuthStore> {
        self.oauth.as_ref()
    }
}

impl FromRef<AppState> for OAuthWebConfig {
    fn from_ref(input: &AppState) -> Self {
        input.config.clone()
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(input: &AppState) -> Self {
        input.key.clone()
    }
}

fn server_metadata(issuer: &str) -> OAuthAuthorizationServerMetadata {
    let mut md = OAuthAuthorizationServerMetadata::default();
    md.issuer = SmolStr::from(issuer);
    md.authorization_endpoint = SmolStr::from(format!("{issuer}/authorize"));
    md.token_endpoint = SmolStr::from(format!("{issuer}/token"));
    md.require_pushed_authorization_requests = Some(true);
    md.pushed_authorization_request_endpoint = Some(SmolStr::from(format!("{issuer}/par")));
    md.token_endpoint_auth_methods_supported = Some(vec![SmolStr::new_static("none")]);
    md.dpop_signing_alg_values_supported = Some(vec![SmolStr::new_static("ES256")]);
    md
}

fn client_data() -> ClientData<SmolStr> {
    ClientData {
        keyset: None,
        config: AtprotoClientMetadata::new_localhost(
            None,
            Some(Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap()),
        ),
    }
}

fn app_state() -> (AppState, MockClient) {
    let client = MockClient::default();
    let oauth =
        OAuthClient::new_from_resolver(MemoryAuthStore::new(), client.clone(), client_data());
    (
        AppState {
            oauth: Arc::new(oauth),
            config: OAuthWebConfig::default(),
            key: Key::generate(),
        },
        client,
    )
}

fn session_data(session_id: &str) -> ClientSessionData {
    let did = Did::new_static("did:plc:alice").unwrap();
    ClientSessionData {
        account_did: did.clone(),
        session_id: SmolStr::from(session_id),
        host_url: Uri::parse("https://pds.example.com").unwrap().to_owned(),
        authserver_url: SmolStr::new_static("https://issuer"),
        authserver_token_endpoint: SmolStr::new_static("https://issuer/token"),
        authserver_revocation_endpoint: None,
        scopes: Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap(),
        dpop_data: DpopClientData {
            dpop_key: jacquard::oauth::utils::generate_key(&[SmolStr::new_static("ES256")])
                .unwrap(),
            dpop_authserver_nonce: SmolStr::default(),
            dpop_host_nonce: SmolStr::default(),
        },
        token_set: TokenSet {
            iss: SmolStr::new_static("https://issuer"),
            sub: did,
            aud: SmolStr::new_static("https://pds.example.com"),
            scope: Some(SmolStr::new_static("atproto rpc:*")),
            refresh_token: Some(SmolStr::new_static("rt")),
            access_token: SmolStr::new_static("atk"),
            token_type: OAuthTokenType::DPoP,
            expires_at: Some(Datetime::raw_str("2099-01-01T00:00:00.000000Z")),
        },
        resolved_scopes: None,
    }
}

async fn strict_handler(
    ExtractOAuthSession(session): ExtractOAuthSession<MockClient, MemoryAuthStore>,
) -> impl IntoResponse {
    let (did, session_id) = session.session_info().await;
    Json(serde_json::json!({ "did": did, "session_id": session_id }))
}

async fn browser_handler(
    BrowserOAuthSession(session): BrowserOAuthSession<MockClient, MemoryAuthStore>,
) -> impl IntoResponse {
    let (did, _) = session.session_info().await;
    Json(serde_json::json!({ "did": did }))
}

#[tokio::test]
async fn metadata_route_serves_client_metadata_from_state_oauth_client() {
    let (state, _) = app_state();
    let expected = atproto_client_metadata(
        &state.oauth.registry.client_data.config,
        &state.oauth.registry.client_data.keyset,
    )
    .unwrap();
    let expected = serde_json::to_value(expected).unwrap();
    let app = routes::<MockClient, MemoryAuthStore, AppState>(&OAuthWebConfig::default())
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let response = server.get("/oauth-client-metadata.json").await;
    response.assert_status_ok();
    let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn strict_extractor_loads_cookie_bound_session() {
    let (state, _) = app_state();
    let data = session_data("cookie-session");
    let key = SessionKey::new(data.account_did.clone(), data.session_id.clone());
    state
        .oauth
        .registry
        .store
        .upsert_session(data)
        .await
        .unwrap();
    let app = Router::new()
        .route("/protected", get(strict_handler))
        .route(
            "/issue",
            get({
                let config = state.config.clone();
                let key = key.clone();
                move |jar| {
                    let config = config.clone();
                    let key = key.clone();
                    async move { set_session_cookie(jar, &config, &key).unwrap() }
                }
            }),
        )
        .with_state(state);
    let server = TestServer::builder().save_cookies().build(app).unwrap();

    server.get("/issue").await.assert_status_ok();
    let response = server.get("/protected").await;
    response.assert_status_ok();
    assert!(response.text().contains("did:plc:alice"));
}

#[tokio::test]
async fn strict_extractor_loads_header_bound_session() {
    let (state, _) = app_state();
    let data = session_data("header-session");
    let key = SessionKey::new(data.account_did.clone(), data.session_id.clone());
    state
        .oauth
        .registry
        .store
        .upsert_session(data)
        .await
        .unwrap();
    let encoded = encode_session_key(&key).unwrap();
    let app = Router::new()
        .route("/protected", get(strict_handler))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/protected")
        .add_header("x-jacquard-session", encoded)
        .await;
    response.assert_status_ok();
}

#[tokio::test]
async fn strict_extractor_rejects_missing_session() {
    let (state, _) = app_state();
    let app = Router::new()
        .route("/protected", get(strict_handler))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    server.get("/protected").await.assert_status_unauthorized();
}

#[tokio::test]
async fn browser_extractor_redirects_missing_session_to_login_with_return_to() {
    let (state, _) = app_state();
    let app = Router::new()
        .route("/protected", get(browser_handler))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let response = server.get("/protected?x=1").await;
    response.assert_status(StatusCode::TEMPORARY_REDIRECT);
    let location = response.header("location");
    let location = location.to_str().unwrap();
    assert!(location.contains("/oauth/login?"));
    assert!(location.contains("return_to=%2Fprotected%3Fx%3D1"));
}

#[tokio::test]
async fn browser_extractor_redirects_deleted_session_to_start_with_did() {
    let (state, _) = app_state();
    let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "deleted-session");
    let app = Router::new()
        .route("/protected", get(browser_handler))
        .route(
            "/issue",
            get({
                let config = state.config.clone();
                let key = key.clone();
                move |jar| {
                    let config = config.clone();
                    let key = key.clone();
                    async move { set_session_cookie(jar, &config, &key).unwrap() }
                }
            }),
        )
        .with_state(state);
    let server = TestServer::builder().save_cookies().build(app).unwrap();

    server.get("/issue").await.assert_status_ok();
    let response = server.get("/protected").await;
    response.assert_status(StatusCode::TEMPORARY_REDIRECT);
    let location = response.header("location");
    let location = location.to_str().unwrap();
    assert!(location.contains("/oauth/start?"));
    assert!(location.contains("identifier=did%3Aplc%3Aalice"));
}

#[tokio::test]
async fn start_auth_query_redirects_to_authorization_endpoint() {
    let (state, client) = app_state();
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::CREATED)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "request_uri": "urn:par:abc",
                        "expires_in": 60
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    let app = routes::<MockClient, MemoryAuthStore, AppState>(&state.config).with_state(state);
    let server = TestServer::new(app).unwrap();

    let response = server
        .get("/oauth/start?identifier=alice.bsky.social&return_to=/protected")
        .await;
    response.assert_status(StatusCode::TEMPORARY_REDIRECT);
    let location = response.header("location");
    let location = location.to_str().unwrap();
    assert!(location.starts_with("https://issuer/authorize?"));
    assert!(location.contains("request_uri=urn%3Apar%3Aabc"));
}

#[tokio::test]
async fn callback_rejects_unknown_state() {
    let (state, _) = app_state();
    let app = routes::<MockClient, MemoryAuthStore, AppState>(&state.config).with_state(state);
    let server = TestServer::new(app).unwrap();

    server
        .get("/oauth/callback?code=abc&state=missing&iss=https%3A%2F%2Fissuer")
        .await
        .assert_status_bad_request();
}

#[tokio::test]
async fn callback_success_sets_session_cookie() {
    let (state, client) = app_state();
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::CREATED)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "request_uri": "urn:par:abc",
                        "expires_in": 60
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header("DPoP-Nonce", http::HeaderValue::from_static("n1"))
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "access_token": "atk1",
                        "token_type": "DPoP",
                        "refresh_token": "rt1",
                        "sub": "did:plc:alice",
                        "iss": "https://issuer",
                        "aud": "https://pds.example.com",
                        "scope": "atproto rpc:*",
                        "expires_in": 3600
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    // Explicit state flow for deterministic callback assertion.
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::CREATED)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "request_uri": "urn:par:def",
                        "expires_in": 60
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header("DPoP-Nonce", http::HeaderValue::from_static("n2"))
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "access_token": "atk2",
                        "token_type": "DPoP",
                        "refresh_token": "rt2",
                        "sub": "did:plc:alice",
                        "iss": "https://issuer",
                        "aud": "https://pds.example.com",
                        "scope": "atproto rpc:*",
                        "expires_in": 3600
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    state
        .oauth
        .start_auth(
            "alice.bsky.social",
            jacquard::oauth::types::AuthorizeOptions::default()
                .with_state(SmolStr::new_static("known-state")),
        )
        .await
        .unwrap();
    let app = routes::<MockClient, MemoryAuthStore, AppState>(&state.config)
        .route("/protected", get(strict_handler))
        .with_state(state);
    let server = TestServer::builder().save_cookies().build(app).unwrap();
    let response = server
        .get("/oauth/callback?code=abc&state=known-state&iss=https%3A%2F%2Fissuer")
        .await;
    response.assert_status(StatusCode::SEE_OTHER);
    let protected = server.get("/protected").await;
    assert_eq!(
        protected.status_code(),
        StatusCode::OK,
        "protected route body: {}",
        protected.text()
    );
}

fn queue_par_response<'a>(
    client: &'a MockClient,
    request_uri: &'static str,
) -> impl Future<Output = ()> + 'a {
    async move {
        client
            .push(
                HttpResponse::builder()
                    .status(StatusCode::CREATED)
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(
                        serde_json::to_vec(&serde_json::json!({
                            "request_uri": request_uri,
                            "expires_in": 60
                        }))
                        .unwrap(),
                    )
                    .unwrap(),
            )
            .await;
    }
}

fn queue_token_response<'a>(
    client: &'a MockClient,
    access_token: &'static str,
) -> impl Future<Output = ()> + 'a {
    async move {
        client
            .push(
                HttpResponse::builder()
                    .status(StatusCode::OK)
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header("DPoP-Nonce", http::HeaderValue::from_static("n-callback"))
                    .body(
                        serde_json::to_vec(&serde_json::json!({
                            "access_token": access_token,
                            "token_type": "DPoP",
                            "refresh_token": "rt-callback",
                            "sub": "did:plc:alice",
                            "iss": "https://issuer",
                            "aud": "https://pds.example.com",
                            "scope": "atproto rpc:*",
                            "expires_in": 3600
                        }))
                        .unwrap(),
                    )
                    .unwrap(),
            )
            .await;
    }
}

fn state_from_return_cookie(response: &axum_test::TestResponse, prefix: &str) -> SmolStr {
    response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|cookie| {
            let name = cookie.split_once('=')?.0;
            let encoded = name.strip_prefix(prefix)?;
            let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
            String::from_utf8(bytes).ok().map(SmolStr::from)
        })
        .expect("state-keyed return cookie")
}

#[tokio::test]
async fn start_auth_return_to_callback_redirects_back_and_cookie_states_do_not_conflict() {
    let (state, client) = app_state();
    let app =
        routes::<MockClient, MemoryAuthStore, AppState>(&state.config).with_state(state.clone());
    let server = TestServer::builder().save_cookies().build(app).unwrap();

    queue_par_response(&client, "urn:par:first").await;
    let first = server
        .get("/oauth/start?identifier=alice.bsky.social&return_to=/first")
        .await;
    first.assert_status(StatusCode::TEMPORARY_REDIRECT);
    let first_state = state_from_return_cookie(&first, state.config.return_cookie_prefix.as_str());

    queue_par_response(&client, "urn:par:second").await;
    let second = server
        .get("/oauth/start?identifier=alice.bsky.social&return_to=/second")
        .await;
    second.assert_status(StatusCode::TEMPORARY_REDIRECT);
    let second_state =
        state_from_return_cookie(&second, state.config.return_cookie_prefix.as_str());
    assert_ne!(first_state, second_state);

    queue_token_response(&client, "atk-second").await;
    let callback = server
        .get(&format!(
            "/oauth/callback?code=abc&state={}&iss=https%3A%2F%2Fissuer",
            second_state
        ))
        .await;
    callback.assert_status(StatusCode::SEE_OTHER);
    assert_eq!(callback.header("location").to_str().unwrap(), "/second");
}

#[tokio::test]
async fn logout_deletes_session_and_clears_cookie() {
    let (state, _) = app_state();
    let data = session_data("logout-session");
    let key = SessionKey::new(data.account_did.clone(), data.session_id.clone());
    state
        .oauth
        .registry
        .store
        .upsert_session(data)
        .await
        .unwrap();
    let app = Router::new()
        .route("/protected", get(strict_handler))
        .route(
            "/oauth/logout",
            post(jacquard_axum::oauth::logout_handler::<MockClient, MemoryAuthStore, AppState>),
        )
        .route(
            "/issue",
            get({
                let config = state.config.clone();
                let key = key.clone();
                move |jar| {
                    let config = config.clone();
                    let key = key.clone();
                    async move { set_session_cookie(jar, &config, &key).unwrap() }
                }
            }),
        )
        .with_state(state.clone());
    let server = TestServer::builder().save_cookies().build(app).unwrap();

    server.get("/issue").await.assert_status_ok();
    server.get("/protected").await.assert_status_ok();
    server
        .post("/oauth/logout")
        .await
        .assert_status(StatusCode::SEE_OTHER);
    assert!(
        state
            .oauth
            .registry
            .store
            .get_session(&key.did, key.session_id.as_str())
            .await
            .unwrap()
            .is_none()
    );
    server.get("/protected").await.assert_status_unauthorized();
    server
        .get("/protected")
        .add_header("x-jacquard-session", encode_session_key(&key).unwrap())
        .await
        .assert_status_unauthorized();
}

#[tokio::test]
async fn custom_config_paths_are_honored_by_routes() {
    // Custom paths must differ from the defaults so the assertions are
    // meaningful: the default paths should 404 while the custom ones work.
    let mut config = OAuthWebConfig::default();
    config.start_auth_path = SmolStr::new_static("/auth/begin");
    config.callback_path = SmolStr::new_static("/auth/done");
    config.logout_path = SmolStr::new_static("/auth/exit");

    let (mut state, client) = app_state();
    state.config = config.clone();

    // The start route needs a PAR response before it can redirect.
    queue_par_response(&client, "urn:par:custom").await;
    let app = routes::<MockClient, MemoryAuthStore, AppState>(&state.config).with_state(state);
    let server = TestServer::new(app).unwrap();

    // The custom start path issues a redirect to the authorization endpoint.
    let response = server.get("/auth/begin?identifier=alice.bsky.social").await;
    response.assert_status(StatusCode::TEMPORARY_REDIRECT);
    let location = response.header("location");
    assert!(
        location
            .to_str()
            .unwrap()
            .starts_with("https://issuer/authorize?")
    );

    // The callback route exists at its custom path (unknown state still 400s,
    // which proves the route is mounted and handled rather than 404).
    server
        .get("/auth/done?code=abc&state=missing&iss=https%3A%2F%2Fissuer")
        .await
        .assert_status_bad_request();

    // The default start path is no longer mounted.
    server
        .get("/oauth/start?identifier=alice.bsky.social")
        .await
        .assert_status_not_found();
}

#[tokio::test]
async fn default_config_metadata_route_remains_fixed() {
    // The client-metadata route is intentionally not configurable.
    let (state, _) = app_state();
    let app = routes::<MockClient, MemoryAuthStore, AppState>(&state.config).with_state(state);
    let server = TestServer::new(app).unwrap();

    server
        .get("/oauth-client-metadata.json")
        .await
        .assert_status_ok();
}
