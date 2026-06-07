#![cfg(all(test, feature = "scope-check"))]

use std::collections::VecDeque;
use std::sync::Arc;

use bytes::Bytes;
use http::{Response as HttpResponse, StatusCode};
use jacquard::client::Agent;
use jacquard::deps::fluent_uri::Uri;
use jacquard::types::did::Did;
use jacquard::types::string::Nsid;
use jacquard::xrpc::XrpcClient;
use jacquard::{BosStr, IntoStatic};
use jacquard_common::http_client::HttpClient;
use jacquard_oauth::atproto::AtprotoClientMetadata;
use jacquard_oauth::client::OAuthSession;
use jacquard_oauth::resolver::OAuthResolver;
use jacquard_oauth::scopes::{
    RepoAction, RepoCollection, RepoScope, RpcAudience, RpcLexicon, RpcScope, Scope, Scopes,
};
use jacquard_oauth::session::SessionRegistry;
use jacquard_oauth::session::{ClientData, ClientSessionData, DpopClientData};
use jacquard_oauth::types::{OAuthAuthorizationServerMetadata, OAuthTokenType, TokenSet};
use smol_str::{SmolStr, format_smolstr};
use std::collections::BTreeSet;
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
            requested: None,
        })
    }
}

impl OAuthResolver for MockClient {
    async fn get_authorization_server_metadata(
        &self,
        issuer: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, jacquard_oauth::resolver::ResolverError> {
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

impl jacquard_oauth::dpop::DpopExt for MockClient {}

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

fn create_session_data(resolved_scopes: Option<Vec<Scope<SmolStr>>>) -> ClientSessionData {
    ClientSessionData {
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
        resolved_scopes,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_scope_check_permits_matching_rpc() {
    let client = Arc::new(MockClient::default());

    // Queue a successful response for getSession
    client.push(get_session_ok()).await;

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-test-matching-{}.json",
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

    // Create resolved_scopes with rpc:com.atproto.server.getSession
    let mut rpc_lexicon = BTreeSet::new();
    rpc_lexicon.insert(RpcLexicon::Nsid(
        Nsid::<SmolStr>::new_static("com.atproto.server.getSession").unwrap(),
    ));
    let mut aud = BTreeSet::new();
    aud.insert(RpcAudience::All);
    let resolved_scopes = Some(vec![Scope::Rpc(RpcScope {
        lxm: rpc_lexicon,
        aud,
    })]);

    let session_data = create_session_data(resolved_scopes).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .expect("xrpc send should succeed with matching scope");
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify HTTP request was made
    let log = client.log.lock().await;
    assert_eq!(log.len(), 1, "expected 1 HTTP call");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_scope_check_denies_ungranted() {
    let client = Arc::new(MockClient::default());

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-test-deny-{}.json",
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

    // Create resolved_scopes with a different rpc scope
    let mut rpc_lexicon = BTreeSet::new();
    rpc_lexicon.insert(RpcLexicon::Nsid(
        Nsid::<SmolStr>::new_static("com.example.other").unwrap(),
    ));
    let mut aud = BTreeSet::new();
    aud.insert(RpcAudience::All);
    let resolved_scopes = Some(vec![Scope::Rpc(RpcScope {
        lxm: rpc_lexicon,
        aud,
    })]);

    let session_data = create_session_data(resolved_scopes).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await;

    // Should error because scope doesn't grant access.
    let err = match resp {
        Err(e) => e,
        Ok(_) => panic!("xrpc send should fail without matching scope"),
    };
    let err_msg = format!("{}", err);

    // Verify the error contains the denied NSID so the developer knows what failed.
    assert!(
        err_msg.contains("com.atproto.server.getSession"),
        "error should mention the denied NSID, got: {err_msg}"
    );

    // Verify the error contains the granted scopes so the developer can diagnose.
    assert!(
        err_msg.contains("com.example.other"),
        "error should mention the granted scopes for diagnostics, got: {err_msg}"
    );

    // Verify NO HTTP request was made (proof that AC7.3 works — scope check
    // short-circuits before the HTTP layer).
    let log = client.log.lock().await;
    assert_eq!(
        log.len(),
        0,
        "no HTTP request should be made when scope check fails"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_scope_check_no_resolved_scopes_permits() {
    let client = Arc::new(MockClient::default());

    // Queue a successful response for getSession
    client.push(get_session_ok()).await;

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-test-none-{}.json",
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

    // Create session with None resolved_scopes (no resolution was done)
    let session_data = create_session_data(None).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .expect("xrpc send should succeed when resolved_scopes is None");
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify HTTP request was made
    let log = client.log.lock().await;
    assert_eq!(log.len(), 1, "expected 1 HTTP call");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_scope_check_wildcard_rpc_permits() {
    let client = Arc::new(MockClient::default());

    // Queue a successful response for getSession
    client.push(get_session_ok()).await;

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-test-wildcard-{}.json",
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

    // Create resolved_scopes with wildcard rpc scope
    let mut rpc_lexicon = BTreeSet::new();
    rpc_lexicon.insert(RpcLexicon::All);
    let mut aud = BTreeSet::new();
    aud.insert(RpcAudience::All);
    let resolved_scopes = Some(vec![Scope::Rpc(RpcScope {
        lxm: rpc_lexicon,
        aud,
    })]);

    let session_data = create_session_data(resolved_scopes).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .expect("xrpc send should succeed with wildcard scope");
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify HTTP request was made
    let log = client.log.lock().await;
    assert_eq!(log.len(), 1, "expected 1 HTTP call");

    let _ = std::fs::remove_file(&path);
}

/// Build the resolved scope set that would result from expanding
/// `include:app.bsky.authCreatePosts` — real permission set from
/// the AT Protocol spec.
///
/// Grants:
/// - rpc: app.bsky.video.{uploadVideo,getJobStatus,getUploadLimits}
/// - repo: app.bsky.feed.{post,postgate,threadgate} (create only)
fn resolved_scopes_auth_create_posts() -> Vec<Scope<SmolStr>> {
    let video_rpcs = [
        "app.bsky.video.uploadVideo",
        "app.bsky.video.getJobStatus",
        "app.bsky.video.getUploadLimits",
    ];
    let collections = [
        "app.bsky.feed.post",
        "app.bsky.feed.postgate",
        "app.bsky.feed.threadgate",
    ];

    let mut scopes = Vec::new();

    // RPC scopes for video endpoints.
    let mut lxm = BTreeSet::new();
    for nsid in &video_rpcs {
        lxm.insert(RpcLexicon::Nsid(Nsid::<SmolStr>::new_static(nsid).unwrap()));
    }
    let mut aud = BTreeSet::new();
    aud.insert(RpcAudience::All);
    scopes.push(Scope::Rpc(RpcScope { lxm, aud }));

    // Repo scopes for post creation (create-only).
    for col in &collections {
        let mut actions = BTreeSet::new();
        actions.insert(RepoAction::Create);
        scopes.push(Scope::Repo(RepoScope {
            collection: RepoCollection::Nsid(Nsid::<SmolStr>::new_static(col).unwrap()),
            actions,
        }));
    }

    scopes
}

#[tokio::test(flavor = "multi_thread")]
async fn test_realistic_scopes_video_rpc_permitted() {
    // Scenario: session has resolved app.bsky.authCreatePosts permissions.
    // Calling app.bsky.video.getUploadLimits should succeed.
    let client = Arc::new(MockClient::default());
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&serde_json::json!({"key": "value"})).unwrap())
                .unwrap(),
        )
        .await;

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-realistic-rpc-{}.json",
        std::process::id(),
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

    let session_data = create_session_data(Some(resolved_scopes_auth_create_posts())).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::app_bsky::video::get_upload_limits::GetUploadLimits)
        .await
        .expect("video RPC should be permitted by authCreatePosts scope");
    assert_eq!(resp.status(), StatusCode::OK);

    let log = client.log.lock().await;
    assert_eq!(log.len(), 1, "expected 1 HTTP call");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_realistic_scopes_ungated_rpc_denied() {
    // Scenario: session has only authCreatePosts permissions.
    // Calling getSession (not in the permission set) should be DENIED.
    let client = Arc::new(MockClient::default());

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-realistic-deny-{}.json",
        std::process::id(),
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

    let session_data = create_session_data(Some(resolved_scopes_auth_create_posts())).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let err = match agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
    {
        Err(e) => e,
        Ok(_) => panic!("getSession should be denied — not in authCreatePosts permission set"),
    };

    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("com.atproto.server.getSession"),
        "error should identify the denied NSID, got: {err_msg}"
    );

    // No HTTP call made — scope check short-circuited.
    let log = client.log.lock().await;
    assert_eq!(log.len(), 0, "no HTTP request when scope check fails");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_realistic_scopes_audience_specific_rpc_permitted() {
    // Critical regression test: granted scopes with a SPECIFIC audience
    // (e.g., did:web:api.bsky.app) must still permit the request.
    // The client doesn't know the target audience at pre-flight time —
    // audience enforcement is the server's responsibility.
    let client = Arc::new(MockClient::default());
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&serde_json::json!({"key": "value"})).unwrap())
                .unwrap(),
        )
        .await;

    let mut path = std::env::temp_dir();
    path.push(format!(
        "jacquard-scope-aud-specific-{}.json",
        std::process::id(),
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

    // Granted scope has a SPECIFIC audience — this is the normal case
    // for inter-service auth in permission sets (e.g., "aud": "did:web:api.bsky.app").
    let mut lxm = BTreeSet::new();
    lxm.insert(RpcLexicon::Nsid(
        Nsid::<SmolStr>::new_static("app.bsky.video.getUploadLimits").unwrap(),
    ));
    let mut aud = BTreeSet::new();
    aud.insert(RpcAudience::Did(
        Did::<SmolStr>::new_static("did:web:api.bsky.app").unwrap(),
    ));
    let resolved_scopes = Some(vec![Scope::Rpc(RpcScope { lxm, aud })]);

    let session_data = create_session_data(resolved_scopes).into_static();
    let client_arc = client.clone();
    let registry = Arc::new(SessionRegistry::new(store, client_arc.clone(), client_data));
    registry.set(session_data.clone()).await.unwrap();
    let session = OAuthSession::new(registry, client_arc, session_data);

    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::app_bsky::video::get_upload_limits::GetUploadLimits)
        .await
        .expect("RPC with audience-specific scope should still be permitted at pre-flight");
    assert_eq!(resp.status(), StatusCode::OK);

    let log = client.log.lock().await;
    assert_eq!(log.len(), 1, "expected 1 HTTP call");
    let _ = std::fs::remove_file(&path);
}
