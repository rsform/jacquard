use std::collections::VecDeque;
use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderValue, Method, Response as HttpResponse, StatusCode};
use jacquard::BosStr;
use jacquard::client::AtpSession;
use jacquard::client::credential_session::{
    CredentialResumeResult, CredentialSession, SessionKey, resolve_credential_session_hint,
};
use jacquard::deps::fluent_uri::Uri;
use jacquard::identity::resolver::{DidDocResponse, IdentityResolver, ResolverOptions};
use jacquard::types::did::Did;
use jacquard::types::string::Handle;
use jacquard::xrpc::XrpcClient;
use jacquard_common::http_client::HttpClient;
use jacquard_common::session::{MemorySessionStore, SessionHint, SessionStore};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone, Default)]
struct MockClient {
    // Queue of HTTP responses to pop for each send_http call
    queue: Arc<Mutex<VecDeque<HttpResponse<Vec<u8>>>>>,
    // Capture requests for assertions
    log: Arc<Mutex<Vec<http::Request<Vec<u8>>>>>,
    // Count calls to identity resolver helpers
    did_doc_calls: Arc<RwLock<usize>>,
    handle_calls: Arc<RwLock<usize>>,
}

impl MockClient {
    async fn push(&self, resp: HttpResponse<Vec<u8>>) {
        self.queue.lock().await.push_back(resp);
    }
    async fn take_log(&self) -> Vec<http::Request<Vec<u8>>> {
        let mut log = self.log.lock().await;
        let out = log.clone();
        log.clear();
        out
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

impl IdentityResolver for MockClient {
    fn options(&self) -> &ResolverOptions {
        use std::sync::LazyLock;
        static OPTS: LazyLock<ResolverOptions> = LazyLock::new(ResolverOptions::default);
        &OPTS
    }

    async fn resolve_handle<S: BosStr + Sync>(
        &self,
        handle: &Handle<S>,
    ) -> std::result::Result<Did, jacquard::identity::resolver::IdentityError> {
        // Return a fixed DID for any handle
        *self.handle_calls.write().await += 1;
        assert!(handle.as_str().contains('.'));
        Ok(Did::new_static("did:plc:alice").unwrap())
    }

    async fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> std::result::Result<DidDocResponse, jacquard::identity::resolver::IdentityError> {
        // Track calls and return a minimal DID doc with a PDS endpoint
        *self.did_doc_calls.write().await += 1;
        assert_eq!(did.as_str(), "did:plc:alice");
        let doc = serde_json::json!({
            "id": "did:plc:alice",
            "service": [{
                "id": "#pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": "https://pds"
            }]
        });
        Ok(DidDocResponse {
            buffer: Bytes::from(serde_json::to_vec(&doc).unwrap()),
            status: StatusCode::OK,
            requested: None,
        })
    }
}

// XrpcExt blanket impl applies via HttpClient

fn create_session_body() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "accessJwt": "acc1",
        "refreshJwt": "ref1",
        "did": "did:plc:alice",
        "handle": "alice.bsky.social"
    }))
    .unwrap()
}

fn refresh_session_body(access: &str, refresh: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "accessJwt": access,
        "refreshJwt": refresh,
        "did": "did:plc:alice",
        "handle": "alice.bsky.social"
    }))
    .unwrap()
}

fn get_session_ok_body() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "did": "did:plc:alice",
        "handle": "alice.bsky.social",
        "active": true
    }))
    .unwrap()
}

fn atp_session(did: &'static str, handle: &'static str) -> AtpSession {
    AtpSession {
        access_jwt: "acc".into(),
        refresh_jwt: "ref".into(),
        did: Did::new_static(did).unwrap(),
        handle: Handle::new_static(handle).unwrap(),
        pds: None,
    }
}

#[tokio::test]
async fn credential_session_hint_matcher_resolves_common_hints() {
    let store = MemorySessionStore::<SessionKey, AtpSession>::default();
    let client = MockClient::default();
    let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "session");
    store
        .set(
            key.clone(),
            atp_session("did:plc:alice", "alice.bsky.social"),
        )
        .await
        .unwrap();

    let matched = resolve_credential_session_hint(&store, &client, &SessionHint::Any)
        .await
        .unwrap()
        .expect("any match");
    assert_eq!(matched.key, key);
    assert_eq!(matched.session.handle.as_str(), "alice.bsky.social");
    assert_eq!(matched.session.pds, None);

    let matched = resolve_credential_session_hint(
        &store,
        &client,
        &SessionHint::Did(Did::new_static("did:plc:alice").unwrap()),
    )
    .await
    .unwrap()
    .expect("did match");
    assert_eq!(matched.key, key);

    let matched = resolve_credential_session_hint(
        &store,
        &client,
        &SessionHint::Handle(Handle::new_static("alice.bsky.social").unwrap()),
    )
    .await
    .unwrap()
    .expect("handle match");
    assert_eq!(matched.key, key);

    let matched = resolve_credential_session_hint(&store, &client, &SessionHint::Key(key.clone()))
        .await
        .unwrap()
        .expect("key match");
    assert_eq!(matched.key, key);

    let missing = resolve_credential_session_hint(
        &store,
        &client,
        &SessionHint::Key(SessionKey::new(
            Did::new_static("did:plc:bob").unwrap(),
            "session",
        )),
    )
    .await
    .unwrap();
    assert!(missing.is_none());

    let identifier = resolve_credential_session_hint(
        &store,
        &client,
        &SessionHint::Identifier("alice@example.com".into()),
    )
    .await
    .unwrap();
    assert!(
        identifier.is_none(),
        "identifier hints must not fall back to Any"
    );
}

#[tokio::test]
async fn credential_resolver_selector_uses_store_before_handle_resolution() {
    let store = MemorySessionStore::<SessionKey, AtpSession>::default();
    let client = MockClient::default();
    let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "session");
    store
        .set(
            key.clone(),
            atp_session("did:plc:alice", "alice.bsky.social"),
        )
        .await
        .unwrap();

    let matched = resolve_credential_session_hint(
        &store,
        &client,
        &SessionHint::Handle(Handle::new_static("alice.bsky.social").unwrap()),
    )
    .await
    .unwrap()
    .expect("store-level handle match");
    assert_eq!(matched.key, key);
    assert_eq!(
        *client.handle_calls.read().await,
        0,
        "store selector should get first chance to satisfy handle hints"
    );

    let matched = resolve_credential_session_hint(
        &store,
        &client,
        &SessionHint::Handle(Handle::new_static("alias.bsky.social").unwrap()),
    )
    .await
    .unwrap()
    .expect("resolver fallback did match");
    assert_eq!(matched.key, key);
    assert_eq!(
        *client.handle_calls.read().await,
        1,
        "resolver should be used only after store selector misses the handle"
    );
}

#[tokio::test]
async fn credential_resume_returns_challenge_details_without_password() {
    let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
    let client = Arc::new(MockClient::default());
    let session = CredentialSession::new(store, client);

    let result = session.resume(&SessionHint::Any).await.expect("resume any");
    let CredentialResumeResult::LoginRequired(challenge) = result else {
        panic!("expected login challenge for empty store");
    };
    assert_eq!(challenge.identifier, None);
    assert_eq!(challenge.session_id, None);

    let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "mobile");
    let result = session
        .resume(&SessionHint::Key(key.clone()))
        .await
        .expect("resume key");
    let CredentialResumeResult::LoginRequired(challenge) = result else {
        panic!("expected login challenge for missing key");
    };
    assert_eq!(challenge.identifier.as_deref(), Some(key.did.as_str()));
    assert_eq!(challenge.session_id.as_deref(), Some("mobile"));

    let result = session
        .resume(&SessionHint::Identifier("alice@example.com".into()))
        .await
        .expect("resume identifier");
    let CredentialResumeResult::LoginRequired(challenge) = result else {
        panic!("expected login challenge for identifier");
    };
    assert_eq!(challenge.identifier.as_deref(), Some("alice@example.com"));
    assert_eq!(challenge.session_id, None);
}

#[tokio::test]
async fn credential_resume_uses_stored_pds_without_resolving_did_doc() {
    let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
    let client = Arc::new(MockClient::default());
    let session = CredentialSession::new(store.clone(), client.clone());
    let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "session");
    let mut stored = atp_session("did:plc:alice", "alice.bsky.social");
    stored.pds = Some(Uri::parse("https://stored-pds").unwrap().to_owned());
    store.set(key.clone(), stored.clone()).await.unwrap();

    let result = session.resume(&SessionHint::Any).await.expect("resume any");
    let CredentialResumeResult::Resumed(resumed) = result else {
        panic!("expected resumed session");
    };
    assert_eq!(resumed.pds.as_ref().unwrap().as_str(), "https://stored-pds");
    assert_eq!(session.endpoint().await.as_str(), "https://stored-pds");
    assert_eq!(*client.did_doc_calls.read().await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn credential_login_and_auto_refresh() {
    let client = Arc::new(MockClient::default());

    // Queue responses in order: createSession 200 → getSession 401 → refreshSession 200 → getSession 200
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(create_session_body())
                .unwrap(),
        )
        .await;
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&serde_json::json!({"error":"ExpiredToken"})).unwrap())
                .unwrap(),
        )
        .await;
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(refresh_session_body("acc2", "ref2"))
                .unwrap(),
        )
        .await;
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(get_session_ok_body())
                .unwrap(),
        )
        .await;

    let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
    let session = CredentialSession::new(store.clone(), client.clone());

    // Before login, default endpoint should be public appview
    assert_eq!(
        session.endpoint().await.as_str(),
        "https://public.api.bsky.app"
    );

    // Login using handle; resolves to PDS and persists session
    session
        .login(
            jacquard::CowStr::from("alice.bsky.social"),
            jacquard::CowStr::from("apppass"),
            Some(jacquard::CowStr::from("session")),
            None,
            None,
            None,
        )
        .await
        .expect("login ok");

    // Endpoint switches to PDS
    assert_eq!(session.endpoint().await.as_str(), "https://pds");

    // Send a request that will first 401 (ExpiredToken), then refresh, then succeed
    let resp = session
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .expect("xrpc send ok");
    assert_eq!(resp.status(), StatusCode::OK);
    let out: jacquard::api::com_atproto::server::get_session::GetSessionOutput<
        jacquard::CowStr<'_>,
    > = resp
        .parse()
        .expect("parse ok after refresh (GetSession output)");
    assert_eq!(out.handle.as_str(), "alice.bsky.social");

    // Verify request sequence and Authorization headers used
    let log = client.take_log().await;
    assert_eq!(log.len(), 4, "expected four HTTP calls");
    // 0: createSession (no auth)
    assert_eq!(log[0].method(), Method::POST);
    assert!(
        log[0]
            .uri()
            .to_string()
            .ends_with("/xrpc/com.atproto.server.createSession")
    );
    assert!(log[0].headers().get(http::header::AUTHORIZATION).is_none());
    // 1: getSession (uses access token acc1)
    assert_eq!(log[1].method(), Method::GET);
    assert!(
        log[1]
            .uri()
            .to_string()
            .ends_with("/xrpc/com.atproto.server.getSession")
    );
    assert_eq!(
        log[1].headers().get(http::header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer acc1"))
    );
    // 2: refreshSession (uses refresh token ref1)
    assert_eq!(log[2].method(), Method::POST);
    assert!(
        log[2]
            .uri()
            .to_string()
            .ends_with("/xrpc/com.atproto.server.refreshSession")
    );
    assert_eq!(
        log[2].headers().get(http::header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer ref1"))
    );
    // 3: getSession (re-sent with new access token acc2)
    assert_eq!(log[3].method(), Method::GET);
    assert!(
        log[3]
            .uri()
            .to_string()
            .ends_with("/xrpc/com.atproto.server.getSession")
    );
    assert_eq!(
        log[3].headers().get(http::header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer acc2"))
    );

    // Verify store updated with refreshed tokens
    let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "session");
    let updated = store.get(&key).await.expect("session present");
    assert_eq!(updated.access_jwt.as_str(), "acc2");
    assert_eq!(updated.refresh_jwt.as_str(), "ref2");
}
