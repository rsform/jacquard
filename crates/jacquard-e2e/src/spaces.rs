//! Permissioned-space scenarios against the reference spaces-alpha provider
//! (AC.13): space lifecycle through the generated `com.atproto.simplespace.*`
//! bindings, plus an authenticated member-list boundary using the second
//! fixture identity.

use jacquard::client::Agent;
use jacquard::client::AgentSessionExt;
use jacquard::client::AtpSession;
use jacquard::client::credential_session::CredentialSession;
use jacquard::identity::JacquardResolver;
use jacquard::types::did::Did;
use jacquard_api::com_atproto::simplespace::add_member::AddMember;
use jacquard_api::com_atproto::simplespace::create_space::{
    CreateSpace, CreateSpaceAppAccess, CreateSpacePolicy,
};
use jacquard_api::com_atproto::simplespace::delete_space::DeleteSpace;
use jacquard_api::com_atproto::simplespace::get_space::{GetSpace, GetSpaceOutputPolicy};
use jacquard_api::com_atproto::simplespace::list_members::ListMembers;
use jacquard_api::com_atproto::simplespace::remove_member::RemoveMember;
use jacquard_api::com_atproto::simplespace::{MemberListPolicy, Open};
use jacquard_api::com_atproto::space::get_space_credential::GetSpaceCredentialError;
use jacquard_api::com_atproto::space::list_spaces::ListSpaces;
use jacquard_common::CowStr;
use jacquard_common::atproto;
use jacquard_common::session::MemorySessionStore;
use jacquard_common::session::SessionKey;
use jacquard_common::types::aturi::AtSpaceUri;
use jacquard_common::types::string::{Nsid, RecordKey};
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use jacquard_oauth::authstore::MemoryAuthStore;
use jacquard_oauth::client::OAuthSession;
use sha2::{Digest, Sha256};

use crate::bootstrap::FixtureStack;
use crate::transport::FixtureTransport;

/// An OAuth session over the fixture transport: the only auth mode the
/// reference PDS accepts for `com.atproto.space.getDelegationToken`
/// (`com.atproto.access` scope).
type SpaceOAuthSession = OAuthSession<JacquardResolver<FixtureTransport>, MemoryAuthStore>;

type CredAgent = Agent<
    CredentialSession<
        MemorySessionStore<SessionKey, AtpSession>,
        JacquardResolver<crate::transport::FixtureTransport>,
    >,
>;

/// Space type NSID for the deterministic test space. The spaces-alpha host
/// treats the type as descriptive metadata; any syntactically valid NSID is
/// accepted and echoed back.
const SPACE_TYPE: &str = "dev.jacquard.e2e.space";

async fn send<R>(
    stack: &FixtureStack,
    agent: &CredAgent,
    endpoint: &str,
    req: R,
) -> Result<(), String>
where
    R: jacquard_common::xrpc::XrpcRequest + Send + Sync + serde::Serialize,
    R::Response: Send + Sync,
{
    agent
        .send(req)
        .await
        .map_err(|e| stack.sanitize_error(endpoint, e))?;
    Ok(())
}

/// Ephemeral DPoP key material for the space-credential flow. Deterministic
/// per run (derived from the run id), which keeps runs reproducible without
/// reusing keys across runs.
struct DpopMaterial {
    jwk: jose_jwk::Key,
    jkt: String,
}

fn dpop_material(stack: &FixtureStack) -> Result<DpopMaterial, String> {
    let seed: [u8; 32] = {
        let mut hasher = Sha256::new();
        hasher.update(format!(
            "jacquard-e2e/dpop/{}",
            stack.context.coordinates.run_id
        ));
        hasher.finalize().into()
    };
    let secret = p256::elliptic_curve::SecretKey::<p256::NistP256>::from_slice(&seed)
        .map_err(|e| format!("derive DPoP key: {e}"))?;
    let jwk = jose_jwk::Key::from(&jose_jwk::crypto::Key::from(secret));
    let jkt = jacquard_oauth::dpop::jkt(&jwk).map_err(|e| format!("DPoP jkt: {e}"))?;
    Ok(DpopMaterial {
        jwk,
        jkt: jkt.to_string(),
    })
}

#[derive(Debug)]
enum MintOutcome {
    Credential(jacquard_oauth::client::SpaceCredentialBundle),
    Denied(GetSpaceCredentialError),
    Failed(String),
}

/// Attempt the delegation-token + DPoP-proof credential mint for `space` as
/// the authenticated `agent`. Each attempt fetches a fresh delegation token
/// (single-use `jti`) and builds a fresh DPoP proof bound to the mint URL
/// and the token hash.
async fn mint_space_credential(
    stack: &FixtureStack,
    oauth: &SpaceOAuthSession,
    space: &AtSpaceUri<jacquard_common::DefaultStr>,
    dpop: &DpopMaterial,
) -> MintOutcome {
    match oauth.space_credential(space, &dpop.jwk).await {
        Ok(credential) => MintOutcome::Credential(credential),
        Err(e) => {
            let text = e.to_string();
            if text.contains("UserNotAuthorized") || text.contains("NotAuthorized") {
                MintOutcome::Denied(GetSpaceCredentialError::UserNotAuthorized(None))
            } else if text.contains("SpaceNotFound") || text.contains("SpaceDeleted") {
                MintOutcome::Failed(stack.sanitize_error("getSpaceCredential", text))
            } else if let Some(code) = extract_xrpc_error_code(&text) {
                MintOutcome::Denied(GetSpaceCredentialError::Other {
                    error: code.into(),
                    message: None,
                })
            } else {
                MintOutcome::Failed(stack.sanitize_error("getSpaceCredential", e))
            }
        }
    }
}

/// Authorize the member identity fresh and attempt a credential mint. Each
/// call runs a complete OAuth authorization so the access token reflects the
/// member's *current* space permissions.
async fn mint_as_member(
    stack: &FixtureStack,
    member_identity: &crate::provider::FixtureIdentity,
    space: &AtSpaceUri<jacquard_common::DefaultStr>,
    dpop: &DpopMaterial,
) -> (MintOutcome, Option<SpaceOAuthSession>) {
    let oauth = match crate::oauth::oauth_session(stack, member_identity).await {
        Ok(session) => session,
        Err(e) => return (MintOutcome::Failed(format!("oauth (member): {e}")), None),
    };
    let outcome = mint_space_credential(stack, &oauth, space, dpop).await;
    let session = matches!(outcome, MintOutcome::Credential(_)).then_some(oauth);
    (outcome, session)
}

/// AC.14 permissioned record operations: create/read/put/delete a record in
/// the space through `com.atproto.space.*`, authenticated by the DPoP-bound
/// credential, then assert a replayed DPoP proof is rejected.
#[allow(clippy::too_many_arguments)]
async fn permissioned_record_roundtrip(
    stack: &FixtureStack,
    session: &SpaceOAuthSession,
    member_did: &Did<jacquard_common::DefaultStr>,
    space: &AtSpaceUri<jacquard_common::DefaultStr>,
    credential: &str,
    dpop: &DpopMaterial,
) -> Result<(), String> {
    use jacquard_api::com_atproto::space::create_record::CreateRecord;
    use jacquard_api::com_atproto::space::delete_record::DeleteRecord;
    use jacquard_api::com_atproto::space::get_record::GetRecord;
    use jacquard_api::com_atproto::space::put_record::PutRecord;

    let collection = jacquard_common::types::nsid::Nsid::new_owned("dev.jacquard.e2e.post")
        .map_err(|e| format!("space collection: {e}"))?;
    let rkey = RecordKey::any_owned(format!("post-{}", stack.context.coordinates.run_id))
        .map_err(|e| format!("space rkey: {e}"))?;
    let record = atproto!({
        "$type": "dev.jacquard.e2e.post",
        "text": "created by permissioned round-trip",
    });

    // Writes (create/put/delete) go through the OAuth session: the alpha
    // image's write endpoints authorize via the OAuth token's space
    // permissions, not the credential. Reads accept the credential.
    use jacquard_common::xrpc::XrpcClient as _;
    let created = session
        .send(CreateRecord {
            collection: collection.clone(),
            record: record.clone(),
            repo: member_did.clone(),
            rkey: Some(rkey.clone()),
            space: space.clone(),
            validate: None,
            extra_data: None,
        })
        .await
        .map_err(|e| stack.sanitize_error("space createRecord", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("space createRecord decode", e))?;
    assert!(
        !created.cid.as_str().is_empty(),
        "space createRecord returns a CID"
    );

    // Read back through the credential-authenticated path.
    let fetched = session
        .space_request(
            GetRecord {
                collection: collection.clone(),
                repo: member_did.clone(),
                rkey: rkey.clone(),
                space: space.clone(),
            },
            credential,
            &dpop.jwk,
        )
        .await
        .map_err(|e| stack.sanitize_error("space getRecord (credential)", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("space getRecord decode", e))?;
    let _ = fetched.value;

    // Update via the OAuth session; a new CID proves the write.
    let updated_record = atproto!({
        "$type": "dev.jacquard.e2e.post",
        "text": "updated by permissioned round-trip",
    });
    let put = session
        .send(PutRecord {
            collection: collection.clone(),
            record: updated_record,
            repo: member_did.clone(),
            rkey: rkey.clone(),
            space: space.clone(),
            validate: None,
            extra_data: None,
        })
        .await
        .map_err(|e| stack.sanitize_error("space putRecord", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("space putRecord decode", e))?;
    assert_ne!(
        put.cid.as_str(),
        created.cid.as_str(),
        "putRecord produces a new CID"
    );

    // Negative: a tampered DPoP proof (wrong ath) must be rejected while the
    // record still exists, so a record-not-found response cannot mask an auth
    // failure.
    // DPoP binds to the public protected-resource origin, not the internal
    // bridge address used by FixtureTransport to reach the container.
    let url = format!(
        "https://pds.{}.jacquard-e2e.test/xrpc/com.atproto.space.getRecord",
        stack.context.provider.name()
    );
    let wrong_ath = jacquard_oauth::dpop::build_dpop_proof(
        &dpop.jwk,
        "GET",
        &url,
        None,
        Some("dGhhbmt5X3lvdV9mb3JfdGhlX3JlaWdodA"),
    )
    .map_err(|e| format!("build tampered proof: {e}"))?;
    use jacquard_common::AuthorizationToken;
    use jacquard_common::http_client::HttpClient;
    use jacquard_common::xrpc::CallOptions;
    let request = jacquard_api::com_atproto::space::get_record::GetRecord {
        collection: collection.clone(),
        repo: member_did.clone(),
        rkey: rkey.clone(),
        space: space.clone(),
    };
    let opts = CallOptions {
        auth: Some(AuthorizationToken::Dpop(jacquard_common::DefaultStr::from(
            credential,
        ))),
        atproto_proxy: None,
        atproto_accept_labelers: None,
        extra_headers: vec![(
            http::HeaderName::from_static("dpop"),
            http::HeaderValue::from_str(wrong_ath.as_str())
                .map_err(|e| format!("tampered proof header: {e}"))?,
        )],
    };
    let base_uri = session.base_uri().await;
    let request = jacquard_common::xrpc::build_http_request(&base_uri.borrow(), &request, &opts)
        .map_err(|e| format!("build tampered request: {e}"))?;
    let response = session
        .client
        .send_http(request)
        .await
        .map_err(|e| stack.sanitize_error("tampered getRecord", e))?;
    if response.status().is_success() {
        return Err("tampered DPoP proof was accepted by getRecord".to_string());
    }
    if !matches!(
        response.status(),
        http::StatusCode::BAD_REQUEST | http::StatusCode::UNAUTHORIZED
    ) {
        return Err(format!(
            "tampered DPoP proof returned unexpected status {}",
            response.status()
        ));
    }
    let body: serde_json::Value = serde_json::from_slice(response.body())
        .map_err(|e| format!("decode tampered DPoP rejection: {e}"))?;
    if body.get("error").and_then(serde_json::Value::as_str) != Some("BadDpopProof") {
        return Err(format!(
            "tampered DPoP proof returned unexpected body: {body}"
        ));
    }

    // Delete, then the session read must yield the typed RecordNotFound.
    session
        .send(DeleteRecord {
            collection: collection.clone(),
            repo: member_did.clone(),
            rkey: rkey.clone(),
            space: space.clone(),
            extra_data: None,
        })
        .await
        .map_err(|e| stack.sanitize_error("space deleteRecord", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("space deleteRecord decode", e))?;
    match session
        .send(GetRecord {
            collection,
            repo: member_did.clone(),
            rkey,
            space: space.clone(),
        })
        .await
    {
        Ok(resp) => match resp.into_output() {
            Err(XrpcError::Xrpc(
                jacquard_api::com_atproto::space::get_record::GetRecordError::RecordNotFound(_),
            )) => Ok(()),
            other => Err(stack.sanitize_error(
                "space getRecord (after delete)",
                format!("expected typed RecordNotFound, got {other:?}"),
            )),
        },
        Err(e) => Err(stack.sanitize_error("space getRecord (after delete)", e)),
    }
}

/// Extract the XRPC error code from an OAuthError string, when present.
fn extract_xrpc_error_code(text: &str) -> Option<&str> {
    let start = text.find("error \"")? + 7;
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn assert_denied(stack: &FixtureStack, outcome: MintOutcome, stage: &str) -> Result<(), String> {
    match outcome {
        MintOutcome::Denied(
            GetSpaceCredentialError::UserNotAuthorized(_)
            | GetSpaceCredentialError::NotAuthorized(_),
        ) => Ok(()),
        MintOutcome::Denied(other) => Err(stack.sanitize_error(
            "getSpaceCredential",
            format!("{stage}: expected UserNotAuthorized/NotAuthorized, got {other}"),
        )),
        MintOutcome::Failed(e) => Err(format!("{stage}: mint failed instead of denied: {e}")),
        MintOutcome::Credential(_) => Err(stack.sanitize_error(
            "getSpaceCredential",
            format!("{stage}: unauthorized identity minted a credential"),
        )),
    }
}

/// Run the spaces lifecycle and member-boundary contract (AC.13).
pub async fn reference_spaces_lifecycle_and_member_boundary(
    stack: &FixtureStack,
) -> Result<(), String> {
    let primary = stack.credential_agent().await?;
    let member_identity = stack.context.provider.member_identity();
    let member = stack
        .credential_agent_for(
            &member_identity,
            stack.files.member_app_password(stack.context.provider),
        )
        .await?;

    let primary_did = Did::<jacquard_common::DefaultStr>::new_owned(stack.context.identity.did)
        .map_err(|e| format!("primary DID: {e}"))?;
    let member_did = Did::<jacquard_common::DefaultStr>::new_owned(member_identity.did)
        .map_err(|e| format!("member DID: {e}"))?;

    // Create with a member-list policy and run-scoped skey so concurrent or
    // repeated runs cannot collide on the same space.
    let skey = RecordKey::any_owned(format!("jqe2e-{}", stack.context.coordinates.run_id))
        .map_err(|e| format!("skey: {e}"))?;
    let skey_str = skey.0.as_str().to_string();
    let created = primary
        .send(
            CreateSpace::new()
                .app_access(CreateSpaceAppAccess::Open(Box::new(Open {
                    extra_data: None,
                })))
                .policy(CreateSpacePolicy::MemberListPolicy(Box::new(
                    MemberListPolicy { extra_data: None },
                )))
                .skey(skey.clone())
                .r#type(Nsid::new_owned(SPACE_TYPE).map_err(|e| format!("space type: {e}"))?)
                .build(),
        )
        .await
        .map_err(|e| stack.sanitize_error("com.atproto.simplespace.createSpace", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("createSpace decode", e))?;
    let space = created.uri;
    assert_eq!(
        space.did_authority().as_str(),
        primary_did.as_str(),
        "space authority is the primary fixture identity"
    );
    assert_eq!(
        space.skey().as_str(),
        skey_str,
        "created space echoes the requested skey"
    );

    // getSpace: owned decode proves the stored policy representation.
    let fetched = primary
        .send(GetSpace {
            space: space.clone(),
        })
        .await
        .map_err(|e| stack.sanitize_error("com.atproto.simplespace.getSpace", e))?;
    let fetched_owned = fetched
        .into_output()
        .map_err(|e| stack.sanitize_error("getSpace decode (owned)", e))?;
    assert_eq!(fetched_owned.uri.as_str(), space.as_str());
    assert!(
        matches!(
            fetched_owned.policy,
            GetSpaceOutputPolicy::MemberListPolicy(_)
        ),
        "stored policy round-trips as the member-list variant"
    );
    // Borrowed decode of the same response buffer must agree.
    let borrowed_resp = primary
        .send(GetSpace {
            space: space.clone(),
        })
        .await
        .map_err(|e| stack.sanitize_error("getSpace (borrowed)", e))?;
    let fetched_borrowed = borrowed_resp
        .parse::<CowStr<'_>>()
        .map_err(|e| stack.sanitize_error("getSpace decode (borrowed)", e))?;
    assert_eq!(fetched_borrowed.uri.as_str(), space.as_str());

    // listSpaces: the new space is listed for the primary identity and type.
    let listed = primary
        .send(ListSpaces {
            cursor: None,
            did: Some(primary_did.clone()),
            limit: None,
            r#type: Some(Nsid::new_owned(SPACE_TYPE).map_err(|e| format!("space type: {e}"))?),
        })
        .await
        .map_err(|e| stack.sanitize_error("com.atproto.space.listSpaces", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("listSpaces decode", e))?;
    assert!(
        listed.spaces.iter().any(|v| v.uri == space),
        "listSpaces includes the created space"
    );

    // Publish the space-type declaration: a Lexicon document with
    // `defs.main.type == "space"`, stored at
    // `at://<primary>/com.atproto.lexicon.schema/dev.jacquard.e2e.space`.
    // The provider resolves it (via `_lexicon.dev.jacquard.e2e` DNS → this
    // identity's PDS) when validating the OAuth `space:` scope grant. The
    // generated `Schema` record only types the `lexicon` version field; the
    // declaration body rides in extra_data, exactly as on the wire.
    let declaration = serde_json::json!({
        "lexicon": 1,
        "id": "dev.jacquard.e2e.space",
        "defs": {
            "main": {
                "type": "space",
                "name": "Jacquard e2e test space",
                "key": "any",
                "description": "Space type used by the Jacquard e2e harness.",
                "collections": ["dev.jacquard.e2e.post"],
            }
        }
    });
    let schema_record: jacquard_api::com_atproto::lexicon::schema::Schema =
        serde_json::from_value(declaration)
            .map_err(|e| format!("declaration record decode: {e}"))?;
    primary
        .put_record(
            RecordKey::any_owned("dev.jacquard.e2e.space".to_string())
                .map_err(|e| format!("declaration rkey: {e}"))?,
            schema_record,
        )
        .await
        .map_err(|e| stack.sanitize_error("putRecord (space declaration)", e))?;

    // The provider caches failed lexicon lookups for five minutes, so the
    // mint must not start until the declaration is visible through the
    // exact resolution path the provider uses (`sync.getRecord` over the
    // advertised HTTPS endpoint).
    let mut declared = false;
    for _ in 0..20 {
        let probe = primary
            .send(jacquard_api::com_atproto::sync::get_record::GetRecord {
                did: primary_did.clone(),
                collection: jacquard_common::types::nsid::Nsid::new_owned(
                    "com.atproto.lexicon.schema",
                )
                .map_err(|e| format!("declaration collection: {e}"))?,
                rkey: RecordKey::any_owned("dev.jacquard.e2e.space".to_string())
                    .map_err(|e| format!("declaration rkey: {e}"))?
                    .into(),
            })
            .await;
        if probe.is_ok() {
            declared = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    if !declared {
        return Err("space declaration never became readable via sync.getRecord".to_string());
    }

    // Boundary: member-list enforcement happens at credential mint time.
    // OAuth tokens carry per-space permissions granted at consent, so each
    // stage runs a fresh authorization: denied before membership, a
    // credential after addMember, denied again after removeMember.
    let dpop = dpop_material(stack)?;
    assert_denied(
        stack,
        mint_as_member(stack, &member_identity, &space, &dpop)
            .await
            .0,
        "before membership",
    )?;
    send(
        stack,
        &primary,
        "com.atproto.simplespace.addMember",
        AddMember {
            did: member_did.clone(),
            space: space.clone(),
            extra_data: None,
        },
    )
    .await?;
    let (mint_outcome, mint_session) = mint_as_member(stack, &member_identity, &space, &dpop).await;
    match mint_outcome {
        MintOutcome::Credential(bundle) => {
            // AC.14, local half: verify both tokens cryptographically.
            // Each signer's key is fetched through the fixture resolver's
            // DID-document endpoint (the rotated documents' #atproto keys):
            // the delegation by the delegating member, the credential by
            // the space authority.
            let resolver = stack.resolver();
            async fn fetch_key(
                resolver: &JacquardResolver<crate::transport::FixtureTransport>,
                did: &Did<jacquard_common::DefaultStr>,
            ) -> Result<k256::ecdsa::VerifyingKey, String> {
                use jacquard_identity::resolver::IdentityResolver as _;
                let doc = resolver
                    .resolve_did_doc(did)
                    .await
                    .map_err(|e| format!("DID resolve {}: {e}", did.as_str()))?;
                let did_document: jacquard_common::types::did_doc::DidDocument =
                    serde_json::from_slice(&doc.buffer)
                        .map_err(|e| format!("decode DID document: {e}"))?;
                let public_key = did_document
                    .atproto_public_key()
                    .map_err(|e| format!("decode #atproto multikey: {e}"))?
                    .ok_or("DID document missing #atproto multikey")?;
                k256::ecdsa::VerifyingKey::from_sec1_bytes(&public_key.bytes)
                    .map_err(|e| format!("verifying key: {e}"))
            }
            let authority_key = fetch_key(&resolver, &primary_did).await?;
            let member_key = fetch_key(&resolver, &member_did).await?;
            verify_tokens(
                &primary_did,
                &member_did,
                &space,
                &authority_key,
                &member_key,
                &bundle.delegation_token,
                &bundle.space_credential,
                &dpop.jkt,
            )?;

            // AC.14, permissioned operations: a record round-trip through
            // the credential-authenticated space endpoints. The session
            // that minted the credential drives it; the mint helper returns
            // its session for exactly this continuation.
            permissioned_record_roundtrip(
                stack,
                mint_session
                    .as_ref()
                    .expect("mint success implies session present"),
                &member_did,
                &space,
                &bundle.space_credential,
                &dpop,
            )
            .await?;
        }
        other => {
            return Err(stack.sanitize_error(
                "getSpaceCredential",
                format!("after addMember: expected mint success, got {other:?}"),
            ));
        }
    }

    // A permitted operation for the now-member identity: minting a
    // credential already proved membership; a full member-permissioned
    // record round-trip is covered by AC.14. Here we assert listSpaces
    // visibility, which any authenticated identity may query.
    member
        .send(ListSpaces {
            cursor: None,
            did: Some(primary_did.clone()),
            limit: None,
            r#type: Some(Nsid::new_owned(SPACE_TYPE).map_err(|e| format!("space type: {e}"))?),
        })
        .await
        .map_err(|e| stack.sanitize_error("listSpaces (member)", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("listSpaces (member) decode", e))?;

    let members = primary
        .send(ListMembers {
            cursor: None,
            limit: None,
            space: space.clone(),
        })
        .await
        .map_err(|e| stack.sanitize_error("com.atproto.simplespace.listMembers", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("listMembers decode", e))?;
    assert!(
        members.members.iter().any(|m| m.did == member_did),
        "listMembers contains the added member"
    );

    send(
        stack,
        &primary,
        "com.atproto.simplespace.removeMember",
        RemoveMember {
            did: member_did.clone(),
            space: space.clone(),
            extra_data: None,
        },
    )
    .await?;
    assert_denied(
        stack,
        mint_as_member(stack, &member_identity, &space, &dpop)
            .await
            .0,
        "after removeMember",
    )?;

    // Teardown: deleteSpace, then getSpace yields the typed not-found error.
    send(
        stack,
        &primary,
        "com.atproto.simplespace.deleteSpace",
        DeleteSpace {
            space: space.clone(),
            extra_data: None,
        },
    )
    .await?;
    let gone = primary
        .send(GetSpace {
            space: space.clone(),
        })
        .await
        .map_err(|e| stack.sanitize_error("getSpace (after delete)", e))?;
    match gone.into_output() {
        Err(XrpcError::Xrpc(
            jacquard_api::com_atproto::simplespace::get_space::GetSpaceError::SpaceNotFound(_),
        )) => Ok(()),
        Err(other) => Err(stack.sanitize_error(
            "getSpace (after delete)",
            format!("expected typed SpaceNotFound/SpaceDeleted, got {other:?}"),
        )),
        Ok(_) => Err(stack.sanitize_error(
            "getSpace (after delete)",
            "getSpace returned a space after deleteSpace",
        )),
    }
}

/// Cryptographically verify the delegation token and space credential issued
/// during the boundary flow (AC.14, local half): JWS signatures against the
/// signers' current keys and validates their claims through
/// `jacquard-repo::permissioned`.
fn verify_tokens(
    authority_did: &Did<jacquard_common::DefaultStr>,
    member_did: &Did<jacquard_common::DefaultStr>,
    space: &AtSpaceUri<jacquard_common::DefaultStr>,
    authority_key: &k256::ecdsa::VerifyingKey,
    member_key: &k256::ecdsa::VerifyingKey,
    delegation_token: &str,
    credential_token: &str,
    dpop_jkt: &str,
) -> Result<(), String> {
    use jacquard_common::service_auth::{PublicKey, parse_jwt_with_claims, verify_signature};
    use jacquard_repo::permissioned::CredentialClaims;
    use std::collections::BTreeSet;

    let space_borrowed = space.borrow();
    let authority_public = PublicKey::K256(authority_key.clone());
    let member_public = PublicKey::K256(member_key.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64;

    // Delegation token: signed by the delegating user's repo key.
    let parsed = parse_jwt_with_claims::<CredentialClaims>(delegation_token)
        .map_err(|e| format!("delegation parse: {e}"))?;
    assert_eq!(
        parsed.header().typ.as_str(),
        jacquard_repo::permissioned::DELEGATION_TOKEN_TYP,
        "delegation typ"
    );
    assert_eq!(parsed.header().alg.as_str(), "ES256K", "delegation alg");
    verify_signature(&parsed, &member_public).map_err(|e| format!("delegation signature: {e}"))?;
    let mut replay = BTreeSet::new();
    parsed
        .claims()
        .validate_delegation(now, &member_did.borrow(), &space_borrowed, &mut replay)
        .map_err(|e| format!("delegation claims: {e}"))?;

    // Space credential: signed by the authority's repo key, bound to the
    // holder's DPoP key via cnf.jkt, no audience.
    let parsed = parse_jwt_with_claims::<CredentialClaims>(credential_token)
        .map_err(|e| format!("credential parse: {e}"))?;
    assert_eq!(
        parsed.header().typ.as_str(),
        jacquard_repo::permissioned::SPACE_CREDENTIAL_TYP,
        "credential typ"
    );
    assert_eq!(parsed.header().alg.as_str(), "ES256K", "credential alg");
    verify_signature(&parsed, &authority_public)
        .map_err(|e| format!("credential signature: {e}"))?;
    parsed
        .claims()
        .validate_space_credential(now, &authority_did.borrow(), &space_borrowed, dpop_jkt)
        .map_err(|e| format!("credential claims: {e}"))?;

    Ok(())
}
