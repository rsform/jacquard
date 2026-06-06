use std::collections::VecDeque;
use std::sync::Arc;

use bytes::Bytes;
use http::{Response as HttpResponse, StatusCode};
use jacquard::BosStr;
use jacquard::client::Agent;
use jacquard::xrpc::XrpcClient;
use jacquard_common::http_client::HttpClient;
use jacquard_common::session::SessionHint;
use jacquard_oauth::atproto::AtprotoClientMetadata;
use jacquard_oauth::authstore::ClientAuthStore;
use jacquard_oauth::client::OAuthClient;
use jacquard_oauth::resolver::OAuthResolver;
use jacquard_oauth::scopes::Scopes;
use jacquard_oauth::session::ClientData;
use smol_str::{SmolStr, format_smolstr};

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
    ) -> impl core::future::Future<
        Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>,
    > + Send {
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
    ) -> std::result::Result<jacquard::types::did::Did, jacquard::identity::resolver::IdentityError>
    {
        Ok(jacquard::types::did::Did::new_static("did:plc:alice").unwrap())
    }
    async fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        _did: &jacquard::types::did::Did<S>,
    ) -> std::result::Result<
        jacquard::identity::resolver::DidDocResponse,
        jacquard::identity::resolver::IdentityError,
    > {
        let doc = serde_json::json!({
            "id": "did:plc:alice",
            "service": [{
                "id": "#pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": "https://pds"
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
    async fn resolve_oauth(
        &self,
        _input: &str,
    ) -> Result<
        (
            jacquard_oauth::types::OAuthAuthorizationServerMetadata,
            Option<jacquard_common::types::did_doc::DidDocument>,
        ),
        jacquard_oauth::resolver::ResolverError,
    > {
        let mut md = jacquard_oauth::types::OAuthAuthorizationServerMetadata::default();
        md.issuer = SmolStr::from("https://issuer");
        md.authorization_endpoint = SmolStr::from("https://issuer/authorize");
        md.token_endpoint = SmolStr::from("https://issuer/token");
        md.require_pushed_authorization_requests = Some(true);
        md.pushed_authorization_request_endpoint = Some(SmolStr::from("https://issuer/par"));
        md.token_endpoint_auth_methods_supported = Some(vec![SmolStr::from("none")]);
        md.dpop_signing_alg_values_supported = Some(vec![SmolStr::from("ES256")]);

        // Simple DID doc pointing to https://pds
        let doc = serde_json::json!({
            "id": "did:plc:alice",
            "service": [{
                "id": "#pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": "https://pds"
            }]
        });
        let buf = Bytes::from(serde_json::to_vec(&doc).unwrap());
        let did_doc: jacquard_common::types::did_doc::DidDocument =
            serde_json::from_slice(&buf).unwrap();
        Ok((md, Some(did_doc)))
    }

    async fn get_authorization_server_metadata(
        &self,
        issuer: &str,
    ) -> Result<
        jacquard_oauth::types::OAuthAuthorizationServerMetadata,
        jacquard_oauth::resolver::ResolverError,
    > {
        let mut md = jacquard_oauth::types::OAuthAuthorizationServerMetadata::default();
        md.issuer = SmolStr::from(issuer);
        md.authorization_endpoint = format_smolstr!("{}/authorize", issuer);
        md.token_endpoint = format_smolstr!("{}/token", issuer);
        md.require_pushed_authorization_requests = Some(true);
        md.pushed_authorization_request_endpoint = Some(format_smolstr!("{}/par", issuer));
        md.token_endpoint_auth_methods_supported = Some(vec![SmolStr::from("none")]);
        md.dpop_signing_alg_values_supported = Some(vec![SmolStr::from("ES256")]);
        Ok(md)
    }

    async fn get_resource_server_metadata(
        &self,
        _pds: &str,
    ) -> Result<
        jacquard_oauth::types::OAuthAuthorizationServerMetadata,
        jacquard_oauth::resolver::ResolverError,
    > {
        let mut md = jacquard_oauth::types::OAuthAuthorizationServerMetadata::default();
        md.issuer = SmolStr::from("https://issuer");
        md.authorization_endpoint = SmolStr::from("https://issuer/authorize");
        md.token_endpoint = SmolStr::from("https://issuer/token");
        md.require_pushed_authorization_requests = Some(true);
        md.pushed_authorization_request_endpoint = Some(SmolStr::from("https://issuer/par"));
        md.token_endpoint_auth_methods_supported = Some(vec![SmolStr::from("none")]);
        md.dpop_signing_alg_values_supported = Some(vec![SmolStr::from("ES256")]);
        Ok(md)
    }
}

impl jacquard_oauth::dpop::DpopExt for MockClient {}

#[cfg(feature = "scope-check")]
impl jacquard_identity::lexicon_resolver::LexiconSchemaResolver for MockClient {
    async fn resolve_lexicon_schema<S: jacquard::BosStr + Sync>(
        &self,
        nsid: &jacquard::types::nsid::Nsid<S>,
    ) -> Result<
        jacquard_identity::lexicon_resolver::ResolvedLexiconSchema<'static>,
        jacquard_identity::lexicon_resolver::LexiconResolutionError,
    > {
        // Return an error for this mock - tests that need include scope resolution
        // should provide their own mock or use real resolver

        use jacquard::IntoStatic;
        Err(
            jacquard_identity::lexicon_resolver::LexiconResolutionError::new(
                jacquard_identity::lexicon_resolver::LexiconResolutionErrorKind::FetchFailed {
                    nsid: nsid.into_static(),
                },
                None,
            ),
        )
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_end_to_end_mock_flow() {
    let client = Arc::new(MockClient::default());
    // Queue responses: PAR 201, token 200, XRPC getSession 200
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
                        "aud": "https://pds",
                        "scope": "atproto rpc:*",
                        "expires_in": 3600
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
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "did": "did:plc:alice",
                        "handle": "alice.bsky.social",
                        "active": true
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;

    // File-backed store for auth state/session
    let mut path = std::env::temp_dir();
    path.push(format!("jacquard-oauth-flow-{}.json", std::process::id()));
    std::fs::write(&path, "{}").unwrap();
    let store = jacquard::client::FileAuthStore::new(&path);

    let client_data: ClientData<_> = ClientData {
        keyset: None,
        config: AtprotoClientMetadata::new_localhost(
            None,
            Some(Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap()),
        ),
    };
    let client_arc = client.clone();
    let oauth = OAuthClient::new_from_resolver(store, (*client_arc).clone(), client_data);

    // Build metadata and call PAR to get an AuthRequestData, then save in store
    let (server_metadata, identity) = client.resolve_oauth("alice.bsky.social").await.unwrap();
    let mut metadata = jacquard_oauth::request::OAuthMetadata {
        server_metadata,
        client_metadata: jacquard_oauth::atproto::atproto_client_metadata(
            &AtprotoClientMetadata::new_localhost(
                None,
                Some(Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap()),
            ),
            &None,
        )
        .unwrap(),
        keyset: None,
    };
    let login_hint = identity.map(|_| SmolStr::from("alice.bsky.social"));
    let auth_req =
        jacquard_oauth::request::par(client.as_ref(), login_hint, None, &mut metadata, None)
            .await
            .unwrap();
    // Construct authorization URL as OAuthClient::start_auth would do
    #[derive(serde::Serialize)]
    struct Parameters {
        client_id: SmolStr,
        request_uri: SmolStr,
    }
    let auth_url = format!(
        "{}?{}",
        metadata.server_metadata.authorization_endpoint,
        serde_html_form::to_string(Parameters {
            client_id: metadata.client_metadata.client_id.clone(),
            request_uri: auth_req.request_uri.clone(),
        })
        .unwrap()
    );
    assert!(auth_url.contains("/authorize?"));
    assert!(auth_url.contains("request_uri"));
    // keep state for the callback
    let state = auth_req.state.clone();
    oauth
        .registry
        .store
        .save_auth_req_info(&auth_req)
        .await
        .unwrap();

    // callback: exchange code, create session
    use jacquard_oauth::types::CallbackParams;
    let session = oauth
        .callback(CallbackParams {
            code: SmolStr::from("code123"),
            state: Some(state.clone()),
            // Callback compares exact string with metadata.issuer. Must match exactly.
            iss: Some(SmolStr::from("https://issuer")),
        })
        .await
        .unwrap();

    // Wrap in Agent and send a resource XRPC call to verify Authorization works
    let agent: Agent<_> = Agent::from(session);
    let resp = agent
        .send(jacquard::api::com_atproto::server::get_session::GetSession)
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn oauth_resume_or_start_auth_rejects_any_without_identity() {
    let client = MockClient::default();
    let store = jacquard::client::FileAuthStore::new({
        let mut path = std::env::temp_dir();
        path.push(format!(
            "jacquard-oauth-any-reject-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, "{}").unwrap();
        path
    });
    let oauth = OAuthClient::new_from_resolver(
        store,
        client,
        ClientData {
            keyset: None,
            config: AtprotoClientMetadata::new_localhost(
                None,
                Some(Scopes::new(SmolStr::new_static("atproto rpc:*")).unwrap()),
            ),
        },
    );

    let err = match oauth
        .resume_or_start_auth(
            &SessionHint::Any,
            jacquard_oauth::types::AuthorizeOptions::<SmolStr>::default(),
        )
        .await
    {
        Ok(_) => panic!("Any cannot start auth without identity"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("cannot start OAuth authorization"));
}
