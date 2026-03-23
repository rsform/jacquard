use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jacquard_common::bos::BosStr;
use smol_str::SmolStr;

use super::{Header, jwt::Claims};

/// Builds the base64url-encoded `header.payload` signing input.
fn signing_input<S: BosStr + serde::Serialize>(
    header: &Header<S>,
    claims: &Claims<&str>,
) -> serde_json::Result<(String, String)> {
    let h = URL_SAFE_NO_PAD.encode(serde_json::to_string(header)?);
    let p = URL_SAFE_NO_PAD.encode(serde_json::to_string(claims)?);
    Ok((h, p))
}

/// Assembles a compact JWS from pre-encoded parts and raw signature bytes.
fn assemble(header: &str, payload: &str, sig: &[u8]) -> SmolStr {
    smol_str::format_smolstr!("{header}.{payload}.{}", URL_SAFE_NO_PAD.encode(sig))
}

/// Creates a compact-serialized signed JWT using ES256 (P-256 ECDSA with SHA-256).
pub fn create_signed_jwt_es256<S: BosStr + serde::Serialize>(
    key: p256::ecdsa::SigningKey,
    header: Header<S>,
    claims: Claims<&str>,
) -> serde_json::Result<SmolStr> {
    use p256::ecdsa::signature::Signer;
    let (h, p) = signing_input(&header, &claims)?;
    let sig: p256::ecdsa::Signature = key.sign(format!("{h}.{p}").as_bytes());
    Ok(assemble(&h, &p, &sig.to_bytes()))
}

/// Creates a compact-serialized signed JWT using ES384 (P-384 ECDSA with SHA-384).
pub fn create_signed_jwt_es384<S: BosStr + serde::Serialize>(
    key: p384::ecdsa::SigningKey,
    header: Header<S>,
    claims: Claims<&str>,
) -> serde_json::Result<SmolStr> {
    use p384::ecdsa::signature::Signer;
    let (h, p) = signing_input(&header, &claims)?;
    let sig: p384::ecdsa::Signature = key.sign(format!("{h}.{p}").as_bytes());
    Ok(assemble(&h, &p, &sig.to_bytes()))
}

/// Creates a compact-serialized signed JWT using ES256K (secp256k1 ECDSA with SHA-256).
pub fn create_signed_jwt_es256k<S: BosStr + serde::Serialize>(
    key: k256::ecdsa::SigningKey,
    header: Header<S>,
    claims: Claims<&str>,
) -> serde_json::Result<SmolStr> {
    use k256::ecdsa::signature::Signer;
    let (h, p) = signing_input(&header, &claims)?;
    let sig: k256::ecdsa::Signature = key.sign(format!("{h}.{p}").as_bytes());
    Ok(assemble(&h, &p, &sig.to_bytes()))
}

/// Creates a compact-serialized signed JWT using EdDSA (Ed25519).
pub fn create_signed_jwt_eddsa<S: BosStr + serde::Serialize>(
    key: ed25519_dalek::SigningKey,
    header: Header<S>,
    claims: Claims<&str>,
) -> serde_json::Result<SmolStr> {
    use ed25519_dalek::Signer;
    let (h, p) = signing_input(&header, &claims)?;
    let sig = key.sign(format!("{h}.{p}").as_bytes());
    Ok(assemble(&h, &p, &sig.to_bytes()))
}
