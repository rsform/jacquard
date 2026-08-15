//! Reference-PDS bootstrap: import the deterministic fixture `did:web`
//! identity through the supplied-DID service-JWT contract.
//!
//! State machine (each step is asserted, not assumed):
//! 1. Derive the deterministic initial secp256k1 signing key.
//! 2. Publish the fixture DID document (initial key, PDS service endpoint)
//!    through the ingress admin endpoint.
//! 3. Sign an ES256K service JWT (`iss=sub=<did>`, `aud=<PDS DID>`,
//!    `lxm=com.atproto.server.createAccount`, short `exp`, `iat`, unique
//!    `jti`) and call `createAccount` with `did`/`handle`/`email`/`password`.
//! 4. The account begins deactivated; `getRecommendedDidCredentials` returns
//!    destination-managed credentials that atomically replace the fixture
//!    document's key and PDS endpoint before `activateAccount`.
//! 5. Authenticate and create the app password consumed by the shared
//!    credential-session scenarios.

use crate::bootstrap::{send_xrpc, write_doc_durably};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jacquard::types::did::Did;
use jacquard_api::com_atproto::identity::get_recommended_did_credentials::GetRecommendedDidCredentials;
use jacquard_api::com_atproto::server::create_account::CreateAccount;
use jacquard_api::com_atproto::server::create_app_password::{AppPassword, CreateAppPassword};
use jacquard_api::com_atproto::server::create_session::CreateSession;
use jacquard_api::com_atproto::server::reserve_signing_key::ReserveSigningKey;
use jacquard_common::DefaultStr;
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_common::types::string::Handle;
use jacquard_common::types::value::Data;
use k256::ecdsa::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::bootstrap::FixtureStack;
use crate::provider::FixtureIdentity;

/// PDS DID configured in `e2e/compose.yml`.
const REFERENCE_PDS_DID: &str = "did:web:pds.reference.jacquard-e2e.test";

/// Deterministic initial signing key: sha256 over a fixed label. Test-only
/// material by construction; never used anywhere outside the fixture.
fn initial_signing_key(identity: &FixtureIdentity) -> SigningKey {
    let digest: [u8; 32] = {
        let mut hasher = Sha256::new();
        hasher.update(format!("jacquard-e2e/initial-key/{}", identity.did));
        hasher.finalize().into()
    };
    SigningKey::from_slice(&digest).expect("derived key material is a valid scalar")
}

/// Multikey `publicKeyMultibase` for a compressed secp256k1 public key:
/// multicodec secp256k1-pub (varint `0xe7 0x01`) + key bytes, base58btc with
/// the `z` prefix. Matches Jacquard's own `PublicKey::decode` convention.
fn multikey_multibase(verifying_key: &VerifyingKey) -> String {
    let mut bytes = vec![0xe7, 0x01];
    bytes.extend_from_slice(&verifying_key.to_encoded_point(true).as_bytes());
    format!("z{}", bs58::encode(bytes).into_string())
}

fn did_document(did: &str, verifying_key: &VerifyingKey, pds_endpoint: &str) -> serde_json::Value {
    serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/multikey/v1",
        ],
        "id": did,
        "verificationMethod": [{
            "id": format!("{did}#atproto"),
            "type": "Multikey",
            "controller": did,
            "publicKeyMultibase": multikey_multibase(verifying_key),
        }],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": pds_endpoint,
        }],
    })
}

fn b64(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

/// Compact ES256K JWT.
fn sign_es256k_jwt(key: &SigningKey, claims: &serde_json::Value) -> String {
    let header = serde_json::json!({"alg": "ES256K", "typ": "JWT"});
    let signing_input = format!(
        "{}.",
        b64(serde_json::to_vec(&header)
            .expect("serialize header")
            .as_slice()),
    ) + &b64(serde_json::to_vec(claims)
        .expect("serialize claims")
        .as_slice());
    let signature: k256::ecdsa::Signature =
        k256::ecdsa::signature::Signer::sign(key, signing_input.as_bytes());
    format!("{signing_input}.{}", b64(&signature.to_bytes()))
}

/// Publish (or atomically replace) the fixture DID document: durable on disk
/// under `<fixture-root>/identities/` (so it survives ingress restarts) and
/// in the running ingress's vhost map through the admin endpoint.
async fn publish_did_document(
    stack: &FixtureStack,
    identity: &FixtureIdentity,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let host = identity.did.trim_start_matches("did:web:").to_string();
    let doc = did_document(
        identity.did,
        verifying_key,
        "https://pds.reference.jacquard-e2e.test",
    );
    crate::bootstrap::write_doc_durably(stack, &host, &doc).await
}

/// Import the fixture identity into a fresh Tranquil volume via the
/// web-external flow, and leave an app password at
/// `<fixture-root>/tranquil/app-password`.
///
/// Contract (verified against Tranquil 0.6.6 source and live image):
/// 1. `POST /xrpc/com.atproto.server.reserveSigningKey` with
///    `{"did": <fixture did>}` → 200 `{signingKey: "did:key:..."}`.
/// 2. Publish the DID document (reserved key, `#atproto_pds` service naming
///    `https://<PDS_HOSTNAME>`) at the did:web location. Tranquil fetches it
///    over plain HTTP via its loopback did:web exception (the DID embeds
///    `127.0.0.1%3A<port>`), and validates the service endpoint and key.
/// 3. `POST createAccount` with `didType: "web-external"`, `did`,
///    `signingKey` → 200 with session tokens.
/// 4. With `DISABLE_ACCOUNT_VERIFICATION_GATE=true`, `createSession` works
///    and `checkAccountStatus` reports the fixture DID and `activated: true`.
/// 5. `createAppPassword` for the shared credential scenarios.
pub async fn ensure_tranquil_account(stack: &FixtureStack) -> Result<(), String> {
    let identity = &stack.context.identity;
    let marker = stack.files.root.join("tranquil").join("bootstrapped");
    let password_file = stack.files.app_password(stack.context.provider);
    if marker.exists() && password_file.exists() {
        return Ok(());
    }

    // 1. Reserve the repo signing key.
    let reserved = send_xrpc(
        stack,
        &ReserveSigningKey::<DefaultStr> {
            did: Some(Did::new_owned(identity.did).map_err(|e| format!("fixture DID: {e}"))?),
            extra_data: None,
        },
        None,
        "com.atproto.server.reserveSigningKey",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("reserveSigningKey decode", e))?;
    if !reserved.signing_key.starts_with("did:key:") {
        return Err(format!(
            "unexpected reserved key format: {}",
            reserved.signing_key
        ));
    }

    // 2. Publish the DID document: reserved key, PDS service endpoint.
    let multibase = reserved
        .signing_key
        .strip_prefix("did:key:")
        .ok_or("reserved key missing did:key prefix")?;
    let doc = serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/multikey/v1",
        ],
        "id": identity.did,
        "verificationMethod": [{
            "id": format!("{}#atproto", identity.did),
            "type": "Multikey",
            "controller": identity.did,
            "publicKeyMultibase": multibase,
        }],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": format!("https://pds.tranquil.jacquard-e2e.test"),
        }],
    });
    let host = did_web_host(identity.did)?;
    write_doc_durably(stack, &host, &doc).await?;

    // 3. createAccount with the web-external extension fields.
    let account_password =
        std::fs::read_to_string(stack.files.root.join("tranquil").join("admin-password"))
            .map_err(|e| format!("read account password: {e}"))?
            .trim()
            .to_string();

    let account = send_xrpc(
        stack,
        &CreateAccount {
            did: Some(Did::new_owned(identity.did).map_err(|e| format!("fixture DID: {e}"))?),
            email: Some(DefaultStr::new(identity.email)),
            handle: Handle::new_owned(identity.handle)
                .map_err(|e| format!("fixture handle: {e}"))?,
            invite_code: None,
            password: Some(DefaultStr::new(&account_password)),
            plc_op: None,
            recovery_key: None,
            verification_code: None,
            verification_phone: None,
            // Tranquil extension fields for the web-external flow.
            extra_data: Some(
                [
                    (
                        SmolStr::new("didType"),
                        Data::from(jacquard_common::CowStr::Owned("web-external".into())),
                    ),
                    (
                        SmolStr::new("signingKey"),
                        Data::from(jacquard_common::CowStr::Owned(reserved.signing_key.clone())),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        },
        None,
        "com.atproto.server.createAccount",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("createAccount decode", e))?;
    if account.did.as_str() != identity.did {
        return Err(format!(
            "createAccount returned a different DID: {}",
            account.did
        ));
    }

    // 4. Session + activation status under the verification gate bypass.
    let session = send_xrpc(
        stack,
        &CreateSession {
            allow_takendown: None,
            auth_factor_token: None,
            identifier: DefaultStr::new(identity.handle),
            password: DefaultStr::new(&account_password),
            extra_data: None,
        },
        None,
        "com.atproto.server.createSession",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("createSession decode", e))?;

    let status = send_xrpc(
        stack,
        &jacquard_api::com_atproto::server::check_account_status::CheckAccountStatus,
        Some(session.access_jwt.as_str()),
        "com.atproto.server.checkAccountStatus",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("checkAccountStatus decode", e))?;
    if !status.activated {
        return Err(format!(
            "account not activated after web-external bootstrap (validDid={})",
            status.valid_did
        ));
    }
    if !status.valid_did {
        return Err("checkAccountStatus reports invalid DID after bootstrap".to_string());
    }

    // 5. App password for the shared scenarios.
    let app_pw_response = send_xrpc(
        stack,
        &CreateAppPassword {
            name: DefaultStr::new("jacquard-e2e"),
            privileged: None,
            extra_data: None,
        },
        Some(session.access_jwt.as_str()),
        "com.atproto.server.createAppPassword",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("createAppPassword decode", e))?;
    let app_pw_json = serde_json::to_value(&app_pw_response)
        .map_err(|e| format!("reserialize app password output: {e}"))?;
    let app_password: AppPassword =
        serde_json::from_value(app_pw_json).map_err(|e| format!("decode app password: {e}"))?;
    std::fs::write(&password_file, format!("{}\n", app_password.password))
        .map_err(|e| format!("write app password: {e}"))?;
    std::fs::write(&marker, "ok\n").map_err(|e| format!("write marker: {e}"))?;
    Ok(())
}

/// Extract the did:web host (decoding any `%3A`-encoded port) from a
/// fixture DID.
fn did_web_host(did: &str) -> Result<String, String> {
    did.strip_prefix("did:web:")
        .map(|s| s.split(':').next().unwrap_or(s).replace("%3A", ":"))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("not a did:web identifier: {did}"))
}

/// Import the fixture identity into a fresh reference PDS volume and leave
/// an app password at `password_file`. Idempotent per identity: an existing
/// marker means a previous process in this run bootstrapped it.
///
/// State machine (each step is asserted, not assumed): initial DID document
/// with the deterministic self-controlled key, service JWT +
/// `createAccount`, DID rotation to the destination-managed recommended
/// credentials, activation, session, app password.
pub async fn ensure_reference_account(stack: &FixtureStack) -> Result<(), String> {
    import_reference_identity(
        stack,
        &stack.context.identity,
        &stack.files.app_password(stack.context.provider),
    )
    .await?;
    // The member identity exists only for authenticated spaces membership
    // boundaries; it rides the same import contract as the primary.
    import_reference_identity(
        stack,
        &stack.context.provider.member_identity(),
        &stack.files.member_app_password(stack.context.provider),
    )
    .await?;
    let marker = stack.files.root.join("reference").join("bootstrapped");
    std::fs::write(&marker, "ok\n").map_err(|e| format!("write marker: {e}"))?;
    Ok(())
}

async fn import_reference_identity(
    stack: &FixtureStack,
    identity: &crate::provider::FixtureIdentity,
    password_file: &std::path::Path,
) -> Result<(), String> {
    // The lifecycle controller pre-seeds the password file with a random
    // placeholder, so idempotence is keyed on a per-identity marker written
    // only after the import completes.
    let label = password_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("app-password");
    let marker = stack
        .files
        .root
        .join("reference")
        .join(format!("{label}-bootstrapped"));
    if marker.exists() && password_file.exists() {
        return Ok(());
    }

    let key = initial_signing_key(identity);
    let verifying = *key.verifying_key();

    // 1. Initial document with the self-controlled key.
    publish_did_document(stack, identity, &verifying).await?;

    // 2. Reserve the repo signing key with the PDS (unauthenticated,
    //    required before createAccount will accept an imported DID).
    let reserved = send_xrpc(
        stack,
        &ReserveSigningKey::<DefaultStr> {
            did: Some(Did::new_owned(identity.did).map_err(|e| format!("fixture DID: {e}"))?),
            extra_data: None,
        },
        None,
        "com.atproto.server.reserveSigningKey",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("reserveSigningKey decode", e))?;
    if !reserved.signing_key.starts_with("did:key:") {
        return Err(format!(
            "unexpected reserved key format: {}",
            reserved.signing_key
        ));
    }

    // 3. Service JWT + createAccount.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64;
    let jti = format!("jqe2e-{}-{}", stack.context.coordinates.run_id, now);
    let claims = serde_json::json!({
        "iss": identity.did,
        "sub": identity.did,
        "aud": REFERENCE_PDS_DID,
        "lxm": "com.atproto.server.createAccount",
        "iat": now,
        "exp": now + 300,
        "jti": jti,
    });
    let jwt = sign_es256k_jwt(&key, &claims);

    let account_password =
        std::fs::read_to_string(stack.files.root.join("reference").join("admin-password"))
            .map_err(|e| format!("read account password: {e}"))?
            .trim()
            .to_string();

    let account = send_xrpc(
        stack,
        &CreateAccount {
            did: Some(Did::new_owned(identity.did).map_err(|e| format!("fixture DID: {e}"))?),
            email: Some(DefaultStr::new(identity.email)),
            handle: Handle::new_owned(identity.handle)
                .map_err(|e| format!("fixture handle: {e}"))?,
            invite_code: None,
            password: Some(DefaultStr::new(&account_password)),
            plc_op: None,
            recovery_key: None,
            verification_code: None,
            verification_phone: None,
            extra_data: None,
        },
        Some(&jwt),
        "com.atproto.server.createAccount",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("createAccount decode", e))?;
    if account.did.as_str() != identity.did {
        return Err(format!(
            "createAccount returned a different DID: {}",
            account.did
        ));
    }
    let access_jwt = account.access_jwt.to_string();

    // 4. Recommended credentials → atomic document replacement.
    let creds = send_xrpc(
        stack,
        &GetRecommendedDidCredentials,
        Some(&access_jwt),
        "com.atproto.identity.getRecommendedDidCredentials",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("getRecommendedDidCredentials decode", e))?;
    let rotated =
        compose_rotated_document(identity.did, &creds.verification_methods, &creds.services)?;
    publish_recommended_doc(stack, identity, &rotated).await?;

    // 5. Activate. The PDS gates activation on the rotated DID document
    // being visible through its (1s-stale) DID cache, so poll until the
    // account status reports a valid DID first.
    let mut valid_did = false;
    for _ in 0..30 {
        let status = send_xrpc(
            stack,
            &jacquard_api::com_atproto::server::check_account_status::CheckAccountStatus,
            Some(&access_jwt),
            "com.atproto.server.checkAccountStatus",
        )
        .await?
        .into_output()
        .map_err(|e| stack.sanitize_error("checkAccountStatus decode", e))?;
        if status.valid_did {
            valid_did = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    if !valid_did {
        return Err("rotated DID document never became visible to the PDS".to_string());
    }
    let _: () = send_xrpc(
        stack,
        &jacquard_api::com_atproto::server::activate_account::ActivateAccount,
        Some(&access_jwt),
        "com.atproto.server.activateAccount",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("activateAccount", e))?;

    // Prove activation actually stuck before declaring success.
    let status = send_xrpc(
        stack,
        &jacquard_api::com_atproto::server::check_account_status::CheckAccountStatus,
        Some(&access_jwt),
        "com.atproto.server.checkAccountStatus (post-activate)",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("checkAccountStatus decode", e))?;
    if !status.activated {
        return Err(format!(
            "account still deactivated after activateAccount (validDid={})",
            status.valid_did
        ));
    }

    // 6. Session + app password for the shared scenarios.
    let session = send_xrpc(
        stack,
        &CreateSession {
            allow_takendown: None,
            auth_factor_token: None,
            identifier: DefaultStr::new(identity.handle),
            password: DefaultStr::new(&account_password),
            extra_data: None,
        },
        None,
        "com.atproto.server.createSession",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("createSession decode", e))?;

    let app_pw_response = send_xrpc(
        stack,
        &CreateAppPassword {
            name: DefaultStr::new("jacquard-e2e"),
            privileged: None,
            extra_data: None,
        },
        Some(session.access_jwt.as_str()),
        "com.atproto.server.createAppPassword",
    )
    .await?
    .into_output()
    .map_err(|e| stack.sanitize_error("createAppPassword decode", e))?;
    let app_pw_json = serde_json::to_value(&app_pw_response)
        .map_err(|e| format!("reserialize app password output: {e}"))?;
    let app_password: AppPassword =
        serde_json::from_value(app_pw_json).map_err(|e| format!("decode app password: {e}"))?;
    std::fs::write(password_file, format!("{}\n", app_password.password))
        .map_err(|e| format!("write app password: {e}"))?;
    std::fs::write(&marker, "ok\n").map_err(|e| format!("write marker: {e}"))?;
    Ok(())
}

/// Compose the rotated DID document from the PDS-recommended verification
/// methods (`{"#atproto": "did:key:..."}`) and services
/// (`{"#atproto_pds": "https://..."}`).
fn compose_rotated_document(
    did: &str,
    verification_methods: &Option<Data>,
    services: &Option<Data>,
) -> Result<serde_json::Value, String> {
    let methods = verification_methods
        .as_ref()
        .and_then(|d| serde_json::to_value(d).ok())
        .ok_or("no verificationMethods in recommended credentials")?;
    let services_json = services
        .as_ref()
        .and_then(|d| serde_json::to_value(d).ok())
        .ok_or("no services in recommended credentials")?;
    // Keys arrive with or without the leading `#` depending on the provider.
    let get_entry = |map: &serde_json::Value, bare: &str| -> Option<String> {
        ["#", ""]
            .iter()
            .find_map(|prefix| map.get(format!("{prefix}{bare}")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let atproto_key =
        get_entry(&methods, "atproto").ok_or("recommended credentials lack #atproto")?;
    // The service entry is an object (`{"type", "endpoint"}`), not a string.
    let pds_endpoint = ["#", ""]
        .iter()
        .find_map(|prefix| services_json.get(format!("{prefix}atproto_pds")))
        .and_then(|service| service.get("endpoint"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| {
            format!(
                "recommended credentials lack #atproto_pds (services={services_json}, methods={methods})"
            )
        })?;
    let multibase = atproto_key
        .strip_prefix("did:key:")
        .ok_or("recommended key is not a did:key")?;
    Ok(serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/multikey/v1",
        ],
        "id": did,
        "verificationMethod": [{
            "id": format!("{did}#atproto"),
            "type": "Multikey",
            "controller": did,
            "publicKeyMultibase": multibase,
        }],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": pds_endpoint,
        }],
    }))
}

/// Replace the fixture DID document with the PDS-recommended one verbatim.
async fn publish_recommended_doc(
    stack: &FixtureStack,
    identity: &FixtureIdentity,
    did_doc: &serde_json::Value,
) -> Result<(), String> {
    let host = identity.did.trim_start_matches("did:web:").to_string();
    write_doc_durably(stack, &host, did_doc).await
}

fn truncate(text: &str) -> String {
    text.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The multikey encoding and ES256K signing must round-trip through
    /// Jacquard's own verification stack, proving the wire formats before
    /// any provider is involved.
    #[test]
    fn multikey_and_jwt_round_trip_through_jacquard_verification() {
        use jacquard_common::service_auth::{parse_jwt, verify_signature};
        use jacquard_common::types::did_doc::DidDocument;

        let identity = &crate::provider::Provider::Reference.primary_identity();
        let key = initial_signing_key(identity);
        let verifying = *key.verifying_key();
        let multibase = multikey_multibase(&verifying);

        // The DID document decodes our multikey into a public key, proving
        // the multibase/multicodec encoding against Jacquard's own decoder.
        let doc_json = did_document(identity.did, &verifying, "https://example");
        let doc: DidDocument = serde_json::from_value(doc_json).expect("parse did doc");
        let methods = doc.verification_method.clone().expect("methods present");
        assert_eq!(
            methods[0].public_key_multibase.as_ref().map(|m| m.as_ref()),
            Some(multibase.as_str())
        );
        let public_key = doc
            .atproto_public_key()
            .expect("decode multikey to public key")
            .expect("key present");

        // And a JWT signed here verifies against that public key.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_secs() as i64;
        let claims = serde_json::json!({
            "iss": identity.did,
            "sub": identity.did,
            "aud": identity.did,
            "iat": now,
            "exp": now + 300,
        });
        let jwt = sign_es256k_jwt(&key, &claims);
        let parsed = parse_jwt(&jwt).expect("parse jwt");
        let verifying_key =
            k256::ecdsa::VerifyingKey::from(public_key.to_k256().expect("decode as secp256k1"));
        let service_key = jacquard_common::service_auth::PublicKey::K256(verifying_key);
        verify_signature(&parsed, &service_key).expect("verify ES256K signature");
    }
}
