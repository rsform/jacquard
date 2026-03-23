use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use elliptic_curve::SecretKey;
use jacquard_common::BosStr;
use jose_jwk::{Key, crypto};
use rand::{CryptoRng, RngCore, rngs::ThreadRng};
use sha2::{Digest, Sha256};
use smol_str::SmolStr;
use std::cmp::Ordering;

use crate::{FALLBACK_ALG, types::OAuthAuthorizationServerMetadata};

/// Generate a fresh JWK secret key using the first algorithm from `allowed_algos` that is
/// supported, returning `None` if none are supported.
///
/// Currently only `ES256` (P-256 ECDSA) is implemented; other algorithm identifiers are skipped.
pub fn generate_key(allowed_algos: &[impl AsRef<str>]) -> Option<Key> {
    for alg in allowed_algos {
        #[allow(clippy::single_match)]
        match alg.as_ref() {
            "ES256" => {
                return Some(Key::from(&crypto::Key::from(
                    SecretKey::<p256::NistP256>::random(&mut ThreadRng::default()),
                )));
            }
            _ => {
                // TODO: Implement other algorithms?
            }
        }
    }
    None
}

/// Generate a cryptographically random 16-byte nonce encoded as base64url (no padding).
pub fn generate_nonce() -> SmolStr {
    URL_SAFE_NO_PAD
        .encode(get_random_values::<_, 16>(&mut ThreadRng::default()))
        .into()
}

/// Generate a cryptographically random 43-byte PKCE code verifier encoded as base64url (no padding).
pub fn generate_verifier() -> SmolStr {
    URL_SAFE_NO_PAD
        .encode(get_random_values::<_, 43>(&mut ThreadRng::default()))
        .into()
}

/// Fill a `LEN`-byte array with cryptographically random bytes from `rng`.
pub fn get_random_values<R, const LEN: usize>(rng: &mut R) -> [u8; LEN]
where
    R: RngCore + CryptoRng,
{
    let mut bytes = [0u8; LEN];
    rng.fill_bytes(&mut bytes);
    bytes
}

/// Compare two algorithm identifier strings by preference order for DPoP key generation.
///
/// The ordering is: ES256K > ES (256 > 384 > 512) > PS (256 > 384 > 512) > RS (256 > 384 > 512) > other.
/// Algorithms within the same family are ordered by key length, preferring shorter (faster) keys first.
pub fn compare_algos(a: &impl AsRef<str>, b: &impl AsRef<str>) -> Ordering {
    let a = a.as_ref();
    let b = b.as_ref();
    if a == "ES256K" {
        return Ordering::Less;
    }
    if b == "ES256K" {
        return Ordering::Greater;
    }
    for prefix in ["ES", "PS", "RS"] {
        if let Some(stripped_a) = a.strip_prefix(prefix) {
            if let Some(stripped_b) = b.strip_prefix(prefix) {
                if let (Ok(len_a), Ok(len_b)) =
                    (stripped_a.parse::<u32>(), stripped_b.parse::<u32>())
                {
                    return len_a.cmp(&len_b);
                }
            } else {
                return Ordering::Less;
            }
        } else if b.starts_with(prefix) {
            return Ordering::Greater;
        }
    }
    Ordering::Equal
}

/// Generate a PKCE challenge/verifier pair.
///
/// Returns `(challenge, verifier)` where `challenge` is the base64url-encoded SHA-256 hash
/// of the verifier, per [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
/// The verifier must be kept secret and sent at the token endpoint; the challenge is sent at
/// the authorization endpoint.
pub fn generate_pkce() -> (SmolStr, SmolStr) {
    // https://datatracker.ietf.org/doc/html/rfc7636#section-4.1
    let verifier = generate_verifier();
    (
        URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_str()))
            .into(),
        verifier,
    )
}

/// Generate a DPoP signing key compatible with the algorithms advertised by the authorization server.
///
/// Reads `dpop_signing_alg_values_supported` from the server metadata, sorts by preference
/// using [`compare_algos`], and attempts to generate a key for the most preferred supported
/// algorithm. Falls back to [`crate::FALLBACK_ALG`] if the server does not advertise any algorithms.
pub fn generate_dpop_key<S: BosStr>(
    metadata: &mut OAuthAuthorizationServerMetadata<S>,
) -> Option<Key> {
    let mut fallback = vec![S::from_static(FALLBACK_ALG)];
    let algs = metadata
        .dpop_signing_alg_values_supported
        .as_deref_mut()
        .unwrap_or(&mut fallback);
    algs.sort_by(compare_algos);
    generate_key(&algs)
}
