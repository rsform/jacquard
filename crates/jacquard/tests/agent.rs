use std::collections::VecDeque;
use std::sync::Arc;

use http::{HeaderValue, Response as HttpResponse, StatusCode};
use jacquard::client::credential_session::{CredentialSession, SessionKey};
use jacquard::client::{Agent, AtpSession};
use jacquard::identity::resolver::{DidDocResponse, IdentityResolver, ResolverOptions};
use jacquard::types::did::Did;
use jacquard::types::string::Handle;
use jacquard_common::http_client::HttpClient;
use jacquard_common::session::MemorySessionStore;
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

impl IdentityResolver for MockClient {
    fn options(&self) -> &ResolverOptions {
        use std::sync::LazyLock;
        static OPTS: LazyLock<ResolverOptions> = LazyLock::new(ResolverOptions::default);
        &OPTS
    }
    async fn resolve_handle(
        &self,
        _handle: &Handle<'_>,
    ) -> std::result::Result<Did<'static>, jacquard::identity::resolver::IdentityError> {
        Ok(Did::new_static("did:plc:alice").unwrap())
    }
    async fn resolve_did_doc(
        &self,
        _did: &Did<'_>,
    ) -> std::result::Result<DidDocResponse, jacquard::identity::resolver::IdentityError> {
        let doc = serde_json::json!({
            "id": "did:plc:alice",
            "service": [{
                "id": "#pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": "https://pds"
            }]
        });
        Ok(DidDocResponse {
            buffer: bytes::Bytes::from(serde_json::to_vec(&doc).unwrap()),
            status: StatusCode::OK,
            requested: None,
        })
    }
}

// XrpcExt blanket impl applies via HttpClient

fn refresh_session_body(access: &str, refresh: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "accessJwt": access,
        "refreshJwt": refresh,
        "did": "did:plc:alice",
        "handle": "alice.bsky.social"
    }))
    .unwrap()
}

#[tokio::test]
async fn agent_delegates_to_session_and_refreshes() {
    let client = Arc::new(MockClient::default());
    let store: Arc<MemorySessionStore<SessionKey, AtpSession>> = Arc::new(Default::default());
    let session = CredentialSession::new(store.clone(), client.clone());

    // Seed a session in the store and activate it via restore (sets endpoint to PDS)
    let atp = AtpSession {
        access_jwt: "acc1".into(),
        refresh_jwt: "ref1".into(),
        did: Did::new_static("did:plc:alice").unwrap(),
        handle: Handle::new_static("alice.bsky.social").unwrap(),
    };
    let key: SessionKey = (atp.did.clone(), "session".into());
    jacquard_common::session::SessionStore::set(store.as_ref(), key.clone(), atp)
        .await
        .unwrap();
    session
        .restore(Did::new_static("did:plc:alice").unwrap(), "session".into())
        .await
        .unwrap();

    let agent: Agent<_> = Agent::from(session);
    assert_eq!(agent.kind(), jacquard::client::AgentKind::AppPassword);
    let info = agent.info().await.expect("session info");
    assert_eq!(info.0.as_str(), "did:plc:alice");
    assert_eq!(info.1.as_ref().unwrap().as_str(), "session");
    assert_eq!(agent.endpoint().await.as_str(), "https://pds/");

    // Queue a refresh response and call agent.refresh(); Authorization header must use refresh token
    client
        .push(
            HttpResponse::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(refresh_session_body("acc2", "ref2"))
                .unwrap(),
        )
        .await;

    let token = agent.refresh().await.expect("refresh ok");
    match token {
        jacquard::AuthorizationToken::Bearer(s) => assert_eq!(s.as_ref(), "acc2"),
        _ => panic!("expected Bearer token"),
    }

    // Validate the refreshSession call used the refresh token header
    let log = client.log.lock().await;
    assert_eq!(log.len(), 1);
    assert!(
        log[0]
            .uri()
            .to_string()
            .ends_with("/xrpc/com.atproto.server.refreshSession")
    );
    assert_eq!(
        log[0].headers().get(http::header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer ref1"))
    );
}
