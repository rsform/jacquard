use std::sync::Arc;

use bytes::Bytes;
use http::{Response as HttpResponse, StatusCode};
use jacquard::BosStr;
use jacquard::client::credential_session::{CredentialSession, SessionKey};
use jacquard::client::{AtpSession, FileAuthStore};
use jacquard::identity::resolver::{DidDocResponse, IdentityResolver, ResolverOptions};
use jacquard::types::did::Did;
use jacquard::types::string::Handle;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::http_client::HttpClient;
use jacquard_common::session::SessionStore;
use std::fs;
use std::path::PathBuf;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
struct MockResolver {
    // Count calls to DID doc resolution
    did_doc_calls: Arc<RwLock<usize>>,
}

impl HttpClient for MockResolver {
    type Error = std::convert::Infallible;
    fn send_http(
        &self,
        _request: http::Request<Vec<u8>>,
    ) -> impl core::future::Future<
        Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>,
    > + Send {
        async {
            // Not used in this test
            Ok(HttpResponse::builder()
                .status(StatusCode::OK)
                .body(Vec::new())
                .unwrap())
        }
    }
}

impl IdentityResolver for MockResolver {
    fn options(&self) -> &ResolverOptions {
        use std::sync::LazyLock;
        static OPTS: LazyLock<ResolverOptions> = LazyLock::new(ResolverOptions::default);
        &OPTS
    }
    async fn resolve_handle<S: BosStr + Sync>(
        &self,
        _handle: &Handle<S>,
    ) -> std::result::Result<Did, jacquard::identity::resolver::IdentityError> {
        Ok(Did::new_static("did:plc:alice").unwrap())
    }
    async fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        _did: &Did<S>,
    ) -> std::result::Result<DidDocResponse, jacquard::identity::resolver::IdentityError> {
        *self.did_doc_calls.write().await += 1;
        let doc = serde_json::json!({
            "@context": ["https://www.w3.org/ns/did/v1"],
            "id": "did:plc:alice",
            "alsoKnownAs": ["at://alice.bsky.social"],
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": "https://pds-resolved.example.com"
            }]
        });
        Ok(DidDocResponse {
            buffer: Bytes::from(serde_json::to_vec(&doc).unwrap()),
            status: StatusCode::OK,
            requested: None,
        })
    }
}

fn temp_file() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("jacquard-test-restore-{}.json", std::process::id()));
    p
}

#[tokio::test]
async fn restore_uses_cached_pds_when_present() {
    let path = temp_file();
    fs::write(&path, "{}").unwrap();
    let store = Arc::new(FileAuthStore::new(&path));
    let resolver = Arc::new(MockResolver::default());

    // Seed an app-password session in the file store
    let session = AtpSession {
        access_jwt: "acc".into(),
        refresh_jwt: "ref".into(),
        did: Did::new_static("did:plc:alice").unwrap(),
        handle: Handle::new_static("alice.bsky.social").unwrap(),
        pds: Some(
            Uri::parse("https://pds-cached")
                .expect("valid uri")
                .to_owned(),
        ),
    };
    let key = SessionKey::new(session.did.clone(), "session");
    jacquard_common::session::SessionStore::set(store.as_ref(), key.clone(), session)
        .await
        .unwrap();
    // Verify it is persisted
    assert!(SessionStore::get(store.as_ref(), &key).await.is_some());
    let session = CredentialSession::new(store.clone(), resolver.clone());
    // Restore should pick cached PDS and NOT call resolve_did_doc
    session
        .restore(Did::new_static("did:plc:alice").unwrap(), "session".into())
        .await
        .expect("restore ok");
    assert_eq!(session.endpoint().await.as_str(), "https://pds-cached");

    // Cleanup
    let _ = fs::remove_file(&path);
}
