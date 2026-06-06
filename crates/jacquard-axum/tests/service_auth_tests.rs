use axum::{
    Extension, Router,
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    middleware,
    routing::get,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use jacquard_axum::service_auth::{
    ExtractOptionalServiceAuth, ExtractServiceAuth, ReplayKey, ReplayStore, ReplayStoreError,
    ServiceAuthConfig, VerifiedServiceAuth, require_service_id, service_auth_middleware,
};
use jacquard_common::{
    bos::BosStr,
    deps::smol_str::{SmolStr, format_smolstr},
    service_auth::JwtHeader,
    types::{
        did::Did,
        did_doc::{DidDocument, VerificationMethod, default_context},
    },
};
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use reqwest::StatusCode as ReqwestStatusCode;
use serde_json::json;
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

// Test helper: create a signed JWT
fn create_test_jwt(
    iss: &str,
    aud: &str,
    exp: i64,
    lxm: Option<&str>,
    signing_key: &k256::ecdsa::SigningKey,
) -> String {
    create_test_jwt_with_jti(
        iss,
        aud,
        exp,
        lxm,
        Some(&format!("test-jti-{}-{}-{}", iss, aud, exp)),
        signing_key,
    )
}

fn create_test_jwt_with_jti(
    iss: &str,
    aud: &str,
    exp: i64,
    lxm: Option<&str>,
    jti: Option<&str>,
    signing_key: &k256::ecdsa::SigningKey,
) -> String {
    use k256::ecdsa::signature::Signer;

    let header = JwtHeader {
        alg: SmolStr::new_static("ES256K"),
        typ: SmolStr::new_static("JWT"),
    };

    let mut claims_json = json!({
        "iss": iss,
        "aud": aud,
        "exp": exp,
        "iat": chrono::Utc::now().timestamp(),
    });

    if let Some(jti_val) = jti {
        claims_json["jti"] = json!(jti_val);
    }

    if let Some(lxm_val) = lxm {
        claims_json["lxm"] = json!(lxm_val);
    }

    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_string(&header).unwrap());
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_string(&claims_json).unwrap());

    let signing_input = format!("{}.{}", header_b64, payload_b64);

    let signature: k256::ecdsa::Signature = signing_key.sign(signing_input.as_bytes());
    let signature_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    format!("{}.{}", signing_input, signature_b64)
}

// Test helper: create DID document with k256 key
fn create_test_did_doc(did: &str, public_key: &k256::ecdsa::VerifyingKey) -> DidDocument {
    use std::collections::BTreeMap;

    // Encode as compressed SEC1
    let encoded_point = public_key.to_encoded_point(true);
    let key_bytes = encoded_point.as_bytes();

    // Multicodec prefix for secp256k1-pub (0xe701)
    let mut multicodec_bytes = vec![0xe7, 0x01];
    multicodec_bytes.extend_from_slice(key_bytes);

    // Multibase encode (base58btc = 'z')
    let multibase_key = multibase::encode(multibase::Base::Base58Btc, &multicodec_bytes);

    DidDocument {
        context: default_context(),
        id: Did::new_owned(did).unwrap(),
        also_known_as: None,
        verification_method: Some(vec![VerificationMethod {
            id: format_smolstr!("{}#atproto", did),
            r#type: SmolStr::new_static("Multikey"),
            controller: Some(SmolStr::from(did)),
            public_key_multibase: Some(SmolStr::from(multibase_key)),
            extra_data: BTreeMap::new(),
        }]),
        service: None,
        extra_data: BTreeMap::new(),
    }
}

// Mock resolver for tests
#[derive(Clone)]
struct MockResolver {
    did_doc: DidDocument,
    options: ResolverOptions,
}

impl MockResolver {
    fn new(did_doc: DidDocument) -> Self {
        Self {
            did_doc,
            options: ResolverOptions::default(),
        }
    }
}

#[derive(Clone, Default)]
struct DeterministicReplayStore {
    entries: Arc<Mutex<HashMap<ReplayKey, i64>>>,
    now: Arc<Mutex<i64>>,
}

impl DeterministicReplayStore {
    fn set_now(&self, now: i64) {
        *self.now.lock().unwrap() = now;
    }
}

impl ReplayStore for DeterministicReplayStore {
    fn check_and_insert(
        &self,
        key: ReplayKey,
        expires_at: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ReplayStoreError>> + Send + '_>> {
        Box::pin(async move {
            let now = *self.now.lock().unwrap();
            let mut entries = self.entries.lock().unwrap();
            if let Some(existing_expires_at) = entries.get(&key) {
                if *existing_expires_at > now {
                    return Err(ReplayStoreError::Replayed);
                }
            }
            entries.insert(key, expires_at);
            Ok(())
        })
    }
}

impl IdentityResolver for MockResolver {
    fn options(&self) -> &ResolverOptions {
        &self.options
    }

    fn resolve_handle<S: BosStr + Sync>(
        &self,
        _handle: &jacquard_common::types::string::Handle<S>,
    ) -> impl Future<Output = Result<Did, IdentityError>> + Send {
        async { Err(IdentityError::handle_resolution_exhausted()) }
    }

    fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        _did: &Did<S>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>> + Send {
        let doc = self.did_doc.clone();
        async move {
            let json = serde_json::to_vec(&doc).unwrap();
            Ok(DidDocResponse {
                buffer: Bytes::from(json),
                status: ReqwestStatusCode::OK,
                requested: Some(doc.id.clone()),
            })
        }
    }
}

#[tokio::test]
async fn test_extractor_with_valid_jwt() {
    // Generate keypair
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    // Create test DID and JWT
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;

    // JWT with lxm
    let jwt = create_test_jwt(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        &signing_key,
    );

    // Create mock resolver
    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    // Create config (default: require_lxm = true)
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    // Create handler
    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    // Create request with JWT
    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    // Send request
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert_eq!(body, format!("Authenticated as {}", user_did));
}

#[tokio::test]
async fn test_extractor_with_expired_jwt() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() - 300; // Expired

    let jwt = create_test_jwt(user_did, service_did, exp, None, &signing_key);

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_extractor_with_wrong_audience() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let wrong_aud = "did:web:other.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;

    let jwt = create_test_jwt(user_did, wrong_aud, exp, None, &signing_key);

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_extractor_missing_auth_header() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_middleware_with_valid_jwt() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;

    // JWT with lxm
    let jwt = create_test_jwt(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        &signing_key,
    );

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    // Create config (default: require_lxm = true)
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(Extension(auth): Extension<VerifiedServiceAuth<'static>>) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(middleware::from_fn_with_state(
            config.clone(),
            service_auth_middleware::<ServiceAuthConfig<MockResolver>>,
        ))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert_eq!(body, format!("Authenticated as {}", user_did));
}

#[tokio::test]
async fn test_require_lxm() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;

    // JWT without lxm
    let jwt = create_test_jwt(user_did, service_did, exp, None, &signing_key);

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    let config =
        ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver).require_lxm(true);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should fail because lxm is required but missing
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_with_lxm_present() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;

    // JWT with lxm
    let jwt = create_test_jwt(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        &signing_key,
    );

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    let config =
        ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver).require_lxm(true);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!(
            "Authenticated as {} for {}",
            auth.did(),
            auth.lxm().unwrap()
        )
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert_eq!(
        body,
        format!(
            "Authenticated as {} for app.bsky.feed.getFeedSkeleton",
            user_did
        )
    );
}

#[tokio::test]
async fn test_legacy_without_lxm() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;

    // JWT without lxm
    let jwt = create_test_jwt(user_did, service_did, exp, None, &signing_key);

    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);

    // Legacy config: lxm not required
    let config =
        ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver).require_lxm(false);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should succeed because lxm is not required
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert_eq!(body, format!("Authenticated as {}", user_did));
}

#[tokio::test]
async fn test_optional_extractor_valid_and_missing_and_invalid() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;
    let jwt = create_test_jwt(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        &signing_key,
    );
    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(ExtractOptionalServiceAuth(auth): ExtractOptionalServiceAuth) -> String {
        auth.map(|auth| auth.did().to_string())
            .unwrap_or_else(|| "anonymous".to_string())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder().uri("/test").body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), "anonymous");

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, "Basic bad")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, "Bearer not-a-jwt")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_replay_rejects_second_presentation() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;
    let jwt = create_test_jwt_with_jti(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        Some("fixed-jti"),
        &signing_key,
    );
    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(ExtractServiceAuth(_auth): ExtractServiceAuth) -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    for expected in [StatusCode::OK, StatusCode::UNAUTHORIZED] {
        let request = Request::builder()
            .uri("/test")
            .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn test_replay_accepts_again_after_expiration() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let now = chrono::Utc::now().timestamp();
    let exp = now + 300;
    let jwt = create_test_jwt_with_jti(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        Some("expiring-jti"),
        &signing_key,
    );
    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);
    let replay_store = DeterministicReplayStore::default();
    replay_store.set_now(now);
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver)
        .with_replay_store(replay_store.clone());

    async fn handler(ExtractServiceAuth(_auth): ExtractServiceAuth) -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    replay_store.set_now(exp + 1);
    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_replay_disabled_allows_missing_and_repeated_jti() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;
    let jwt = create_test_jwt_with_jti(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        None,
        &signing_key,
    );
    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver)
        .disable_replay_protection();

    async fn handler(ExtractServiceAuth(_auth): ExtractServiceAuth) -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    for _ in 0..2 {
        let request = Request::builder()
            .uri("/test")
            .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn test_missing_jti_rejected_when_replay_enabled() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;
    let jwt = create_test_jwt_with_jti(
        user_did,
        service_did,
        exp,
        Some("app.bsky.feed.getFeedSkeleton"),
        None,
        &signing_key,
    );
    let did_doc = create_test_did_doc(user_did, verifying_key);
    let resolver = MockResolver::new(did_doc);
    let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver);

    async fn handler(ExtractServiceAuth(_auth): ExtractServiceAuth) -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);
    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_global_service_id_allow_list() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;
    let did_doc = create_test_did_doc(user_did, verifying_key);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        auth.aud().to_string()
    }

    for (aud, allowed, expected) in [
        (service_did, false, StatusCode::OK),
        (
            "did:web:feedgen.example.com#bsky_appview",
            false,
            StatusCode::OK,
        ),
        (service_did, true, StatusCode::OK),
        (
            "did:web:feedgen.example.com#bsky_appview",
            true,
            StatusCode::OK,
        ),
        (
            "did:web:feedgen.example.com#other",
            true,
            StatusCode::UNAUTHORIZED,
        ),
        (
            "did:web:other.example.com#bsky_appview",
            true,
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        let resolver = MockResolver::new(did_doc.clone());
        let mut config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver)
            .disable_replay_protection();
        if allowed {
            config = config.allow_service("bsky_appview");
        }
        let app = Router::new()
            .route("/test", get(handler))
            .with_state(config);
        let jwt = create_test_jwt(
            user_did,
            aud,
            exp,
            Some("app.bsky.feed.getFeedSkeleton"),
            &signing_key,
        );
        let request = Request::builder()
            .uri("/test")
            .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected, "aud={aud} allowed={allowed}");
    }
}

#[tokio::test]
async fn test_route_scoped_service_id_policy() {
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let user_did = "did:plc:test123";
    let service_did = "did:web:feedgen.example.com";
    let exp = chrono::Utc::now().timestamp() + 300;
    let did_doc = create_test_did_doc(user_did, verifying_key);

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        auth.service().unwrap_or("bare").to_string()
    }

    for (aud, strict, expected) in [
        (service_did, false, StatusCode::OK),
        (service_did, true, StatusCode::UNAUTHORIZED),
        (
            "did:web:feedgen.example.com#bsky_appview",
            true,
            StatusCode::OK,
        ),
        (
            "did:web:feedgen.example.com#other",
            true,
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        let resolver = MockResolver::new(did_doc.clone());
        let config = ServiceAuthConfig::new(Did::new_static(service_did).unwrap(), resolver)
            .disable_replay_protection();
        let route = get(handler);
        let app = if strict {
            Router::new()
                .route("/test", route)
                .route_layer(require_service_id("bsky_appview"))
                .with_state(config)
        } else {
            Router::new().route("/test", route).with_state(config)
        };
        let jwt = create_test_jwt(
            user_did,
            aud,
            exp,
            Some("app.bsky.feed.getFeedSkeleton"),
            &signing_key,
        );
        let request = Request::builder()
            .uri("/test")
            .header(header::AUTHORIZATION, format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected, "aud={aud} strict={strict}");
    }
}

#[tokio::test]
async fn test_invalid_signature() {
    // Real JWT token from did:plc:uc7pehijmk5jrllip4cglxdd with bogus signature
    let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NksifQ.eyJpYXQiOjE3NjAzOTMyMzUsImlzcyI6ImRpZDpwbGM6dWM3cGVoaWptazVqcmxsaXA0Y2dseGRkIiwiYXVkIjoiZGlkOndlYjpkZXYucGRzbW9vdmVyLmNvbSIsImV4cCI6MTc2MDM5MzI5NSwibHhtIjoiY29tLnBkc21vb3Zlci5iYWNrdXAuc2lnblVwIiwianRpIjoiMTk0MDQzMzQyNmMyNTNlZjhmNmYxZDJjZWE1YzI0NGMifQ.h5BrgYE";

    // Real DID document for did:plc:uc7pehijmk5jrllip4cglxdd
    let did_doc_json = r##"{
        "id": "did:plc:uc7pehijmk5jrllip4cglxdd",
        "alsoKnownAs": ["at://bailey.skeetcentral.com"],
        "verificationMethod": [{
            "controller": "did:plc:uc7pehijmk5jrllip4cglxdd",
            "id": "did:plc:uc7pehijmk5jrllip4cglxdd#atproto",
            "publicKeyMultibase": "zQ3shNBS3N4EB3vX5G1HoxFkS8tDLFXUHaV85rHQZgVM88rM5",
            "type": "Multikey"
        }],
        "service": [{
            "id": "#atproto_pds",
            "serviceEndpoint": "https://skeetcentral.com",
            "type": "AtprotoPersonalDataServer"
        }]
    }"##;

    let did_doc: DidDocument = serde_json::from_str(did_doc_json).unwrap();
    let resolver = MockResolver::new(did_doc);

    let config = ServiceAuthConfig::new(
        Did::new_static("did:web:dev.pdsmoover.com").unwrap(),
        resolver,
    );

    async fn handler(ExtractServiceAuth(auth): ExtractServiceAuth) -> String {
        format!("Authenticated as {}", auth.did())
    }

    let app = Router::new()
        .route("/test", get(handler))
        .with_state(config);

    let request = Request::builder()
        .uri("/test")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should fail due to invalid signature
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
