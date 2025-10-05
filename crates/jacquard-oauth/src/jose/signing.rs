use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jacquard_common::CowStr;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};

use super::{Header, jwt::Claims};

pub fn create_signed_jwt(
    key: SigningKey,
    header: Header,
    claims: Claims,
) -> serde_json::Result<CowStr<'static>> {
    let header = URL_SAFE_NO_PAD.encode(serde_json::to_string(&header)?);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_string(&claims)?);
    let signature: Signature = key.sign(format!("{header}.{payload}").as_bytes());
    Ok(format!(
        "{header}.{payload}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    )
    .into())
}
