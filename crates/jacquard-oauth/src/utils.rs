use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use elliptic_curve::SecretKey;
use jacquard_common::CowStr;
use jose_jwk::{Key, crypto};
use rand::{CryptoRng, RngCore, rngs::ThreadRng};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

use crate::{FALLBACK_ALG, types::OAuthAuthorizationServerMetadata};

pub fn generate_key(allowed_algos: &[CowStr]) -> Option<Key> {
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

pub fn generate_nonce() -> CowStr<'static> {
    URL_SAFE_NO_PAD
        .encode(get_random_values::<_, 16>(&mut ThreadRng::default()))
        .into()
}

pub fn generate_verifier() -> CowStr<'static> {
    URL_SAFE_NO_PAD
        .encode(get_random_values::<_, 43>(&mut ThreadRng::default()))
        .into()
}

pub fn get_random_values<R, const LEN: usize>(rng: &mut R) -> [u8; LEN]
where
    R: RngCore + CryptoRng,
{
    let mut bytes = [0u8; LEN];
    rng.fill_bytes(&mut bytes);
    bytes
}

// 256K > ES (256 > 384 > 512) > PS (256 > 384 > 512) > RS (256 > 384 > 512) > other (in original order)
pub fn compare_algos(a: &CowStr, b: &CowStr) -> Ordering {
    if a.as_ref() == "ES256K" {
        return Ordering::Less;
    }
    if b.as_ref() == "ES256K" {
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

pub fn generate_pkce() -> (CowStr<'static>, CowStr<'static>) {
    // https://datatracker.ietf.org/doc/html/rfc7636#section-4.1
    let verifier = generate_verifier();
    (
        URL_SAFE_NO_PAD
            .encode(Sha256::digest(&verifier.as_str()))
            .into(),
        verifier,
    )
}

pub fn generate_dpop_key(metadata: &OAuthAuthorizationServerMetadata) -> Option<Key> {
    let mut algs = metadata
        .dpop_signing_alg_values_supported
        .clone()
        .unwrap_or(vec![FALLBACK_ALG.into()]);
    algs.sort_by(compare_algos);
    generate_key(&algs)
}
