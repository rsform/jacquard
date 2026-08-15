//! Provider-specific Tranquil OAuth scenario target. Gated behind the
//! `tranquil` feature so the lifecycle controller runs it only against the
//! Tranquil provider.

use std::sync::Arc;

use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::FromRef;
use axum::http::{Request, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use axum_extra::extract::cookie::Key;
use jacquard::client::{Agent, AgentSessionExt};
use jacquard::identity::JacquardResolver;
use jacquard::xrpc::{XrpcClient, XrpcExt};
use jacquard_api::app_bsky::feed::post::Post;
use jacquard_api::com_atproto::server::get_service_auth::GetServiceAuth;
use jacquard_axum::oauth::{
    ExtractOAuthSession, OAuthWebConfig, OAuthWebState, routes as oauth_routes,
};
use jacquard_codegen_tests::pretty::test_blobby;
use jacquard_common::AuthorizationToken;
use jacquard_common::deps::chrono;
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_common::types::string::{AtUri, Datetime, Did, RecordKey};
use jacquard_oauth::authstore::MemoryAuthStore;
use jacquard_oauth::client::OAuthSession;
use tower::ServiceExt;

use jacquard_e2e::bootstrap::FixtureStack;
use jacquard_e2e::oauth::FixtureOAuthClient;
use jacquard_e2e::provider::{Provider, TRANQUIL_SERVICE_DID};
use jacquard_e2e::transport::FixtureTransport;

type TranquilOAuthSession = OAuthSession<JacquardResolver<FixtureTransport>, MemoryAuthStore>;

#[derive(Clone)]
struct OAuthAppState {
    oauth: Arc<FixtureOAuthClient>,
    config: OAuthWebConfig,
    key: Key,
}

impl OAuthWebState<JacquardResolver<FixtureTransport>, MemoryAuthStore> for OAuthAppState {
    fn oauth_client(&self) -> &FixtureOAuthClient {
        self.oauth.as_ref()
    }
}

impl FromRef<OAuthAppState> for OAuthWebConfig {
    fn from_ref(state: &OAuthAppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<OAuthAppState> for Key {
    fn from_ref(state: &OAuthAppState) -> Self {
        state.key.clone()
    }
}

async fn protected(
    ExtractOAuthSession(session): ExtractOAuthSession<
        JacquardResolver<FixtureTransport>,
        MemoryAuthStore,
    >,
) -> impl IntoResponse {
    let (did, session_id) = session.session_info().await;
    Json(serde_json::json!({ "did": did, "session_id": session_id }))
}

fn stack() -> FixtureStack {
    let stack = FixtureStack::from_env().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        stack.context.provider,
        Provider::Tranquil,
        "tranquil oauth scenarios target the tranquil provider"
    );
    stack
}

/// AC.5: a real Tranquil OAuth authorization driven through Jacquard's
/// start/callback APIs yields a usable DPoP-bound session; authenticated
/// record/blob calls work; refresh persists; logout invalidates access.
#[tokio::test]
async fn tranquil_oauth_axum_session_lifecycle() {
    let stack = stack();
    stack
        .ensure_bootstrapped()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let identity = stack.context.identity;

    // 1. Full flow: PAR → JSON authorize → (consent) → Axum callback →
    // private cookie → strict session extraction.
    let authorization = jacquard_e2e::oauth::tranquil_oauth_authorization(&stack, &identity)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let mut config = OAuthWebConfig::default();
    config.callback_path = "/oauth-callback".into();
    config.after_callback_redirect = "/protected".into();
    config.after_logout_redirect = None;
    let state = OAuthAppState {
        oauth: Arc::new(authorization.client),
        config: config.clone(),
        key: Key::generate(),
    };
    let app =
        oauth_routes::<JacquardResolver<FixtureTransport>, MemoryAuthStore, OAuthAppState>(&config)
            .route("/protected", get(protected))
            .with_state(state.clone());

    let query = serde_html_form::to_string(&authorization.callback)
        .expect("serialize provider callback parameters");
    let callback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/oauth-callback?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("route callback request");
    assert_eq!(callback.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        callback.headers().get(header::LOCATION).unwrap(),
        "/protected"
    );
    let cookie = callback
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("jacquard_oauth_session="))
        .and_then(|value| value.split(';').next())
        .expect("callback issues the private session cookie")
        .to_string();

    let protected_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("route protected request");
    assert_eq!(protected_response.status(), StatusCode::OK);
    let body = to_bytes(protected_response.into_body(), 16 * 1024)
        .await
        .expect("read protected response");
    let restored: serde_json::Value =
        serde_json::from_slice(&body).expect("decode protected response");
    assert_eq!(restored["did"], identity.did);
    let session_id = restored["session_id"]
        .as_str()
        .expect("protected response contains a session id");
    let did =
        Did::<jacquard_common::DefaultStr>::new_owned(identity.did).expect("fixture DID is valid");
    let session: TranquilOAuthSession = state
        .oauth
        .restore(&did, session_id)
        .await
        .expect("restore the cookie-bound session from the shared registry");
    let agent = Agent::new(session);

    // 2. The provider-minted service token reaches the native generated route
    // through Tranquil's atproto-proxy implementation.
    let service_request = GetServiceAuth {
        aud: SmolStr::new(TRANQUIL_SERVICE_DID),
        exp: Some(chrono::Utc::now().timestamp() + 300),
        lxm: Some("test.blobby.get".parse().expect("fixture NSID")),
    };
    let service_token = agent
        .send_with_opts(
            service_request,
            jacquard_common::xrpc::CallOptions {
                auth: None,
                atproto_proxy: None,
                atproto_accept_labelers: None,
                extra_headers: Vec::new(),
            },
        )
        .await
        .expect("getServiceAuth request")
        .into_output()
        .expect("decode service token")
        .token;
    let base = jacquard_common::deps::fluent_uri::Uri::parse("https://service.jacquard-e2e.dev")
        .expect("service base URI");
    let proxied = stack
        .transport
        .xrpc(base)
        .auth(AuthorizationToken::Bearer(service_token.clone()))
        .proxy(SmolStr::new(
            TRANQUIL_SERVICE_DID.to_string() + "#" + "jacquard_e2e",
        ))
        .send(&test_blobby::get::Get {
            uri: AtUri::<jacquard_common::DefaultStr>::new_owned(
                "at://did:example:service/test.blobby.record/native",
            )
            .expect("service record URI"),
        })
        .await
        .expect("service proxy request reaches ingress")
        .into_output()
        .expect("decode proxied generated output");
    assert_eq!(proxied.value.note.as_deref(), Some("service proxy"));
    let extra = proxied
        .extra_data
        .as_ref()
        .expect("verified claims in output");
    assert_eq!(extra["serviceDid"].as_str(), Some(identity.did));
    assert_eq!(extra["audience"].as_str(), Some(TRANQUIL_SERVICE_DID));
    assert!(
        !extra.contains_key("service"),
        "Tranquil service JWT has no audience fragment"
    );
    assert_eq!(extra["lxm"].as_str(), Some("test.blobby.get"));
    assert!(extra["jti"].as_str().is_some());

    let replay = stack
        .direct_service_request(
            service_token.as_str(),
            "https://service.jacquard-e2e.dev/xrpc/test.blobby.get?uri=at%3A%2F%2Fdid%3Aexample%3Aservice%2Ftest.blobby.record%2Fnative",
        )
        .await
        .expect("direct service-auth replay request reaches ingress");
    assert_eq!(replay.status(), http::StatusCode::UNAUTHORIZED);
    let replay_body: serde_json::Value =
        serde_json::from_slice(replay.body()).expect("service-auth replay response is JSON");
    assert_eq!(replay_body["error"], "AuthenticationRequired");
    assert!(
        replay_body["message"]
            .as_str()
            .is_some_and(|message| message.contains("replay"))
    );
    // 2. Session identity matches the fixture identity.
    let (did, _handle) = agent.info().await.expect("oauth session reports identity");
    assert_eq!(
        did.as_str(),
        identity.did,
        "session DID is the fixture identity"
    );

    // 3. OAuth-authenticated record round-trip.
    let post = |text: &str| Post::new().text(text).created_at(Datetime::now()).build();
    let created = agent
        .create_record(post("oauth lifecycle"), None)
        .await
        .unwrap_or_else(|e| panic!("createRecord (oauth): {e}"));
    let reread = agent
        .get_record::<Post, _>(&created.uri)
        .await
        .unwrap_or_else(|e| panic!("getRecord (oauth): {e}"))
        .into_output()
        .expect("decode oauth getRecord");
    assert_eq!(reread.value.text.as_str(), "oauth lifecycle");

    // 4. OAuth-authenticated blob round-trip.
    let payload = b"jacquard-e2e oauth blob".to_vec();
    let mime = jacquard_common::types::blob::MimeType::new("text/plain");
    let blob = agent
        .upload_blob(payload.clone(), mime)
        .await
        .unwrap_or_else(|e| panic!("uploadBlob (oauth): {e}"));
    assert_eq!(blob.size, payload.len());
    assert!(!blob.cid().as_str().is_empty(), "uploadBlob returns a CID");

    // 5. Deterministic refresh: move the stored expiry into Jacquard's
    // refresh window, refresh against Tranquil's real token endpoint, and
    // prove the new access token works.
    let session = agent.inner();
    let (did, session_id) = session.session_info().await;
    let mut stored = session
        .registry
        .get(&did, &session_id, false)
        .await
        .expect("stored session present");
    let previous_access_token = stored.token_set.access_token.clone();
    let previous_refresh_token = stored.token_set.refresh_token.clone();
    stored.token_set.expires_at = Some(Datetime::new(
        (chrono::Utc::now() + chrono::Duration::seconds(1)).fixed_offset(),
    ));
    session
        .registry
        .set(stored)
        .await
        .expect("persist forced expiry");

    let refreshed = session
        .refresh()
        .await
        .unwrap_or_else(|e| panic!("refresh: {e}"));
    let persisted = session
        .registry
        .get(&did, &session_id, false)
        .await
        .expect("refreshed session persisted");
    assert_ne!(
        persisted.token_set.access_token, previous_access_token,
        "refresh replaces the access token"
    );
    assert_eq!(
        refreshed,
        session.access_token().await,
        "refresh returns the access token installed in the live session"
    );
    assert_eq!(
        persisted.token_set,
        session.data.read().await.token_set,
        "the store and live session contain the exact refreshed token set"
    );
    assert!(
        previous_refresh_token.is_none() || persisted.token_set.refresh_token.is_some(),
        "refresh does not discard an issued refresh token"
    );
    let reread2 = agent
        .get_record::<Post, _>(&created.uri)
        .await
        .unwrap_or_else(|e| panic!("getRecord (after refresh): {e}"))
        .into_output()
        .expect("decode oauth getRecord (after refresh)");
    assert_eq!(reread2.value.text.as_str(), "oauth lifecycle");

    // 6. Cleanup the record (best-effort), then logout: the local session
    // is gone and continued calls must fail rather than silently work.
    let rkey = RecordKey::any_owned(
        created
            .uri
            .rkey()
            .map(|r| r.as_str().to_string())
            .unwrap_or_default(),
    )
    .expect("rkey echoed back by provider");
    agent
        .delete_record::<Post>(rkey)
        .await
        .expect("delete oauth record");

    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/logout")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("route logout request");
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert!(
        logout
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|value| {
                value.starts_with("jacquard_oauth_session=")
                    && (value.contains("Max-Age=0") || value.contains("Expires="))
            }),
        "logout clears the browser session cookie"
    );
    assert!(
        session
            .registry
            .get(&did, &session_id, false)
            .await
            .is_err(),
        "logout deletes the local session"
    );
    let stale_cookie = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("route stale-cookie request");
    assert_eq!(
        stale_cookie.status(),
        StatusCode::UNAUTHORIZED,
        "the cleared cookie cannot restore the logged-out session"
    );
    assert!(
        agent.get_record::<Post, _>(&created.uri).await.is_err(),
        "the logged-out session cannot continue making authenticated calls"
    );
}
