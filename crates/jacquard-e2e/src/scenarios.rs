//! Shared provider-neutral scenarios: credential record CRUD and blob round
//! trip (AC.3, AC.4).

use jacquard::client::credential_session::CredentialSession;
use jacquard::client::{Agent, AgentSessionExt};
use jacquard_api::app_bsky::feed::post::Post;
use jacquard_api::com_atproto::sync::get_blob::{GetBlob, GetBlobOutput};
use jacquard_codegen_tests::pretty::test_blobby::record::{
    Record as BlobbyRecord, RecordRecord as BlobbyCollection,
};
use jacquard_common::CowStr;
use jacquard_common::session::{MemorySessionStore, SessionKey};
use jacquard_common::types::blob::MimeType;
use jacquard_common::types::collection::RecordError;
use jacquard_common::types::string::{Datetime, RecordKey};
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use jacquard_lexicon::schema::LexiconSchema;

use crate::bootstrap::FixtureStack;

/// Small deterministic payload that satisfies the curated record's
/// `image/png`-accepted, ≤1000-byte primary blob constraint.
const BLOB_PAYLOAD: &[u8] = b"\x89PNG\r\n\x1a\njacquard-e2e deterministic blob";

type AtpSession = jacquard::client::AtpSession;
pub type CredentialAgent = Agent<
    CredentialSession<
        MemorySessionStore<SessionKey, AtpSession>,
        jacquard::identity::JacquardResolver<crate::transport::FixtureTransport>,
    >,
>;

/// Run the typed record CRUD contract (AC.3): create, owned and borrowed
/// read, update, delete, and typed missing-record error, against a real
/// `jacquard-api` record shape.
pub async fn credential_record_crud_contract(stack: &FixtureStack) -> Result<(), String> {
    let agent = stack.credential_agent().await?;
    let (did, handle) = agent
        .info()
        .await
        .expect("authenticated session reports identity");
    assert_eq!(
        did.as_str(),
        stack.context.identity.did,
        "session DID is the fixture identity"
    );
    let _ = handle;

    let post = |text: &str| Post::new().text(text).created_at(Datetime::now()).build();

    // app.bsky.feed.post uses tid keys; let the PDS mint one.
    let created = agent
        .create_record(post("created"), None)
        .await
        .map_err(|e| {
            let detail = match e.source_err() {
                Some(src) => format!(": {src}"),
                None => String::new(),
            };
            stack.sanitize_error("createRecord", format!("{e}{detail}"))
        })?;
    let uri = created.uri.clone();

    // Owned read and borrowed read must agree on the record value.
    let owned = agent
        .get_record::<Post, _>(&uri)
        .await
        .map_err(|e| stack.sanitize_error("getRecord (owned)", e))?;
    let owned_value = owned
        .into_output()
        .map_err(|e| stack.sanitize_error("getRecord (owned decode)", e))?;
    assert_eq!(owned_value.value.text.as_str(), "created");

    let borrowed = agent
        .get_record::<Post, _>(&uri)
        .await
        .map_err(|e| stack.sanitize_error("getRecord (borrowed)", e))?;
    let borrowed_value = borrowed
        .parse::<CowStr<'_>>()
        .map_err(|e| stack.sanitize_error("getRecord (borrowed decode)", e))?;
    assert_eq!(borrowed_value.value.text.as_str(), "created");

    // Update: the re-read value must change and putRecord returns a CID.
    // The rkey comes from the created URI (the PDS minted a TID).
    let rkey = RecordKey::any_owned(
        uri.rkey()
            .map(|r| r.as_str().to_string())
            .unwrap_or_default(),
    )
    .expect("created URI carries an rkey");
    let put = agent
        .put_record(rkey.clone(), post("updated"))
        .await
        .map_err(|e| stack.sanitize_error("putRecord", e))?;
    assert!(!put.cid.as_str().is_empty(), "putRecord returns a CID");
    let reread = agent
        .get_record::<Post, _>(&uri)
        .await
        .map_err(|e| stack.sanitize_error("getRecord (after put)", e))?;
    let reread_value = reread
        .parse::<CowStr<'_>>()
        .map_err(|e| stack.sanitize_error("getRecord (after put decode)", e))?;
    assert_eq!(reread_value.value.text.as_str(), "updated");

    // Delete, then assert the typed missing-record error. Providers differ
    // in transport shape: the reference PDS returns 400 with a typed XRPC
    // body (parsed via `into_output()`); Tranquil returns 404 with the same
    // `RecordNotFound` code in the body (surfaced as `ClientError::Http`).
    agent
        .delete_record::<Post>(rkey)
        .await
        .map_err(|e| stack.sanitize_error("deleteRecord", e))?;
    match agent.get_record::<Post, _>(&uri).await {
        Err(err) => {
            let detail = err
                .source_err()
                .map(|src| src.to_string())
                .unwrap_or_default();
            let all = format!("{err}: {detail}");
            assert!(
                all.contains("404") && all.contains("RecordNotFound"),
                "expected 404 RecordNotFound, got: {all}"
            );
        }
        Ok(resp) => match resp.into_output() {
            Err(XrpcError::Xrpc(RecordError::RecordNotFound(_))) => {}
            Err(other) => {
                return Err(stack.sanitize_error(
                    "getRecord (after delete)",
                    format!("expected typed RecordNotFound, got {other:?}"),
                ));
            }
            Ok(_) => {
                return Err(
                    "getRecord returned a record after deleteRecord (repo was not updated)"
                        .to_string(),
                );
            }
        },
    }
    Ok(())
}

/// Run the blob round-trip contract (AC.4): upload deterministic bytes,
/// embed the returned `BlobRef` in the curated generated record, read it back
/// with exact blob metadata, download the blob, and compare bytes.
pub async fn blob_record_roundtrip_contract(stack: &FixtureStack) -> Result<(), String> {
    let agent = stack.credential_agent().await?;
    let (did, _) = agent
        .info()
        .await
        .expect("authenticated session reports identity");

    let blob = agent
        .upload_blob(BLOB_PAYLOAD.to_vec(), MimeType::new("image/png"))
        .await
        .map_err(|e| stack.sanitize_error("uploadBlob", e))?;
    assert_eq!(blob.size, BLOB_PAYLOAD.len());
    assert_eq!(blob.mime_type.as_str(), "image/png");
    let uploaded_cid = blob.cid().as_str().to_string();
    assert!(!uploaded_cid.is_empty());

    // Embed in the curated generated record and create/read it back.
    let rkey = RecordKey::any_owned(format!(
        "jacquard-e2e-blob-{}",
        stack.context.coordinates.run_id
    ))
    .map_err(|e| format!("rkey: {e}"))?;
    let record = BlobbyRecord {
        primary: blob.into(),
        secondary: None,
        note: None,
        extra_data: None,
    };
    record
        .validate()
        .map_err(|e| format!("fixture record failed its own constraints: {e}"))?;
    let created = agent
        .create_record(record, Some(rkey))
        .await
        .map_err(|e| stack.sanitize_error("createRecord (blob)", e))?;
    let uri = created.uri.clone();

    let reread = agent
        .get_record::<BlobbyRecord, _>(&uri)
        .await
        .map_err(|e| stack.sanitize_error("getRecord (blob record)", e))?;
    let reread_value = reread
        .parse::<CowStr<'_>>()
        .map_err(|e| stack.sanitize_error("getRecord (blob record decode)", e))?;
    let primary = reread_value.value.primary.blob();
    assert_eq!(primary.mime_type.as_str(), "image/png");
    assert_eq!(primary.size, BLOB_PAYLOAD.len());
    assert_eq!(
        primary.cid().as_str(),
        uploaded_cid,
        "blob CID survives the record round trip"
    );

    // Fetch the blob through the typed getBlob path and compare bytes.
    let downloaded = agent
        .send(GetBlob {
            cid: primary.cid().clone(),
            did: did.convert(),
        })
        .await
        .map_err(|e| stack.sanitize_error("com.atproto.sync.getBlob", e))?
        .into_output()
        .map_err(|e| stack.sanitize_error("getBlob decode", e))?;
    let GetBlobOutput { body } = downloaded;
    assert_eq!(
        body.as_ref(),
        BLOB_PAYLOAD,
        "downloaded blob bytes match exactly"
    );

    // Clean up the record; the blob itself is unreferenced afterwards.
    let rkey = RecordKey::any_owned(
        uri.rkey()
            .map(|r| r.as_str().to_string())
            .unwrap_or_default(),
    )
    .expect("rkey echoed back by provider");
    agent
        .delete_record::<BlobbyCollection>(rkey)
        .await
        .map_err(|e| stack.sanitize_error("deleteRecord (blob)", e))?;
    Ok(())
}
