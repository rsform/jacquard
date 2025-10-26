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
    ExtractServiceAuth, ServiceAuthConfig, VerifiedServiceAuth, service_auth_middleware,
};
use jacquard_common::{
    CowStr, IntoStatic,
    service_auth::JwtHeader,
    types::{
        did::Did,
        did_doc::{DidDocument, VerificationMethod},
    },
};
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use reqwest::StatusCode as ReqwestStatusCode;
use serde_json::json;
use std::future::Future;
use tower::ServiceExt;

// Test helper: create a signed JWT
fn create_test_jwt(
    iss: &str,
    aud: &str,
    exp: i64,
    lxm: Option<&str>,
    signing_key: &k256::ecdsa::SigningKey,
) -> String {
    use k256::ecdsa::signature::Signer;

    let header = JwtHeader {
        alg: CowStr::new_static("ES256K"),
        typ: CowStr::new_static("JWT"),
    };

    let mut claims_json = json!({
        "iss": iss,
        "aud": aud,
        "exp": exp,
        "iat": chrono::Utc::now().timestamp(),
    });

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
fn create_test_did_doc(did: &str, public_key: &k256::ecdsa::VerifyingKey) -> DidDocument<'static> {
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
        id: Did::new_owned(did).unwrap().into_static(),
        also_known_as: None,
        verification_method: Some(vec![VerificationMethod {
            id: CowStr::Owned(format!("{}#atproto", did).into()),
            r#type: CowStr::new_static("Multikey"),
            controller: Some(CowStr::Owned(did.into())),
            public_key_multibase: Some(CowStr::Owned(multibase_key.into())),
            extra_data: BTreeMap::new(),
        }]),
        service: None,
        extra_data: BTreeMap::new(),
    }
}

// Mock resolver for tests
#[derive(Clone)]
struct MockResolver {
    did_doc: DidDocument<'static>,
    options: ResolverOptions,
}

impl MockResolver {
    fn new(did_doc: DidDocument<'static>) -> Self {
        Self {
            did_doc,
            options: ResolverOptions::default(),
        }
    }
}

impl IdentityResolver for MockResolver {
    fn options(&self) -> &ResolverOptions {
        &self.options
    }

    fn resolve_handle(
        &self,
        _handle: &jacquard_common::types::string::Handle<'_>,
    ) -> impl Future<Output = Result<Did<'static>, IdentityError>> + Send {
        async { Err(IdentityError::invalid_well_known()) }
    }

    fn resolve_did_doc(
        &self,
        _did: &Did<'_>,
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
