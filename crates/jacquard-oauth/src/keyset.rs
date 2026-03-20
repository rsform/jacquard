use crate::jose::jws::RegisteredHeader;
use crate::jose::jwt::Claims;
use crate::jose::signing;
use jacquard_common::CowStr;
use jose_jwa::{Algorithm, Signing};
use jose_jwk::{Class, EcCurves, OkpCurves, crypto};
use jose_jwk::{Jwk, JwkSet, Key};
use std::collections::HashSet;
use thiserror::Error;

/// Errors that can occur when constructing or using a [`Keyset`].
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Two keys in the set share the same `kid`, which would make key selection ambiguous.
    #[error("duplicate kid: {0}")]
    DuplicateKid(String),
    /// A keyset with no keys cannot sign anything.
    #[error("keys must not be empty")]
    EmptyKeys,
    /// Each key must carry a `kid` so it can be referenced in JWS headers.
    #[error("key at index {0} must have a `kid`")]
    EmptyKid(usize),
    /// No key in the set matches any of the requested signing algorithms.
    #[error("no signing key found for algorithms: {0:?}")]
    NotFound(Vec<Signing>),
    /// Only secret (private) keys may be used for signing; a public key was provided.
    #[error("key for signing must be a secret key")]
    PublicKey,
    /// The key type or curve is not supported for signing.
    #[error("unsupported key type for signing")]
    UnsupportedKey,
    /// The private key (`d` parameter) is missing from the JWK.
    #[error("missing private key material")]
    MissingPrivateKey,
    /// An error from the underlying JWK cryptographic operation.
    #[error("crypto error: {0:?}")]
    JwkCrypto(crypto::Error),
    /// The raw key bytes have an invalid length or format.
    #[error("invalid key material: {0}")]
    InvalidKey(String),
    /// JSON serialization of a JWT header or claims payload failed.
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

/// Convenience result type for keyset operations.
pub type Result<T> = core::result::Result<T, Error>;

/// Signing algorithm preference order for AT Protocol OAuth.
///
/// EdDSA and ES256K are preferred for their security properties, followed by
/// the NIST curves. This order matches common AT Protocol server expectations.
const PREFERRED_SIGNING_ALGORITHMS: [Signing; 4] = [
    Signing::EdDsa,
    Signing::Es256K,
    Signing::Es256,
    Signing::Es384,
];

/// A validated collection of JWK secret keys used for signing DPoP proofs and client assertions.
///
/// Key selection follows [`PREFERRED_SIGNING_ALGORITHMS`] when multiple keys match.
/// Supported algorithms: EdDSA (Ed25519), ES256K (secp256k1), ES256 (P-256), ES384 (P-384).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keyset(Vec<Jwk>);

impl Keyset {
    /// Returns a [`JwkSet`] containing the public halves of all keys in this keyset.
    pub fn public_jwks(&self) -> JwkSet {
        let mut keys = Vec::with_capacity(self.0.len());
        for mut key in self.0.clone() {
            match key.key {
                Key::Ec(ref mut ec) => {
                    ec.d = None;
                }
                Key::Okp(ref mut okp) => {
                    okp.d = None;
                }
                _ => {}
            }
            keys.push(key);
        }
        JwkSet { keys }
    }

    /// Signs a JWT with the best available key that matches one of the requested algorithms.
    ///
    /// Returns [`Error::NotFound`] if no key in the keyset supports any of the given algorithms.
    pub fn create_jwt(&self, algs: &[Signing], claims: Claims) -> Result<CowStr<'static>> {
        let Some(jwk) = self.find_key(algs, Class::Signing) else {
            return Err(Error::NotFound(algs.to_vec()));
        };
        self.create_jwt_with_key(jwk, claims)
    }

    fn find_key(&self, algs: &[Signing], cls: Class) -> Option<&Jwk> {
        let candidates = self
            .0
            .iter()
            .filter_map(|key| {
                if key.prm.cls.is_some_and(|c| c != cls) {
                    return None;
                }
                let alg = alg_for_key(&key.key)?;
                Some((alg, key)).filter(|(alg, _)| algs.contains(alg))
            })
            .collect::<Vec<_>>();
        for pref_alg in PREFERRED_SIGNING_ALGORITHMS {
            for (alg, key) in &candidates {
                if *alg == pref_alg {
                    return Some(key);
                }
            }
        }
        None
    }

    fn create_jwt_with_key(&self, key: &Jwk, claims: Claims) -> Result<CowStr<'static>> {
        let kid = key.prm.kid.clone().unwrap();
        match &key.key {
            Key::Ec(ec) => {
                let d = ec.d.as_ref().ok_or(Error::MissingPrivateKey)?;
                let d_bytes: &[u8] = d.as_ref();
                match ec.crv {
                    EcCurves::P256 => {
                        let signing_key = p256::ecdsa::SigningKey::from_bytes(d_bytes.into())
                            .map_err(|e| Error::InvalidKey(e.to_string()))?;
                        let mut header = RegisteredHeader::from(Algorithm::Signing(Signing::Es256));
                        header.kid = Some(kid.into());
                        Ok(signing::create_signed_jwt_es256(
                            signing_key,
                            header.into(),
                            claims,
                        )?)
                    }
                    EcCurves::P384 => {
                        let signing_key = p384::ecdsa::SigningKey::from_bytes(d_bytes.into())
                            .map_err(|e| Error::InvalidKey(e.to_string()))?;
                        let mut header = RegisteredHeader::from(Algorithm::Signing(Signing::Es384));
                        header.kid = Some(kid.into());
                        Ok(signing::create_signed_jwt_es384(
                            signing_key,
                            header.into(),
                            claims,
                        )?)
                    }
                    EcCurves::P256K => {
                        let signing_key = k256::ecdsa::SigningKey::from_bytes(d_bytes.into())
                            .map_err(|e| Error::InvalidKey(e.to_string()))?;
                        let mut header =
                            RegisteredHeader::from(Algorithm::Signing(Signing::Es256K));
                        header.kid = Some(kid.into());
                        Ok(signing::create_signed_jwt_es256k(
                            signing_key,
                            header.into(),
                            claims,
                        )?)
                    }
                    _ => Err(Error::UnsupportedKey),
                }
            }
            Key::Okp(okp) => match okp.crv {
                OkpCurves::Ed25519 => {
                    let d = okp.d.as_ref().ok_or(Error::MissingPrivateKey)?;
                    let d_bytes: &[u8] = d.as_ref();
                    let signing_key = ed25519_dalek::SigningKey::try_from(d_bytes)
                        .map_err(|e| Error::InvalidKey(e.to_string()))?;
                    let mut header = RegisteredHeader::from(Algorithm::Signing(Signing::EdDsa));
                    header.kid = Some(kid.into());
                    Ok(signing::create_signed_jwt_eddsa(
                        signing_key,
                        header.into(),
                        claims,
                    )?)
                }
                _ => Err(Error::UnsupportedKey),
            },
            _ => Err(Error::UnsupportedKey),
        }
    }
}

/// Returns the signing algorithm for the given JWK key type, if supported.
fn alg_for_key(key: &Key) -> Option<Signing> {
    match key {
        Key::Ec(ec) => match ec.crv {
            EcCurves::P256 => Some(Signing::Es256),
            EcCurves::P384 => Some(Signing::Es384),
            EcCurves::P256K => Some(Signing::Es256K),
            _ => None,
        },
        Key::Okp(okp) => match okp.crv {
            OkpCurves::Ed25519 => Some(Signing::EdDsa),
            _ => None,
        },
        _ => None,
    }
}

/// Parses a string-based algorithm name into a [`Signing`] variant, if it maps to
/// an algorithm this crate supports.
pub fn parse_signing_alg(s: &str) -> Option<Signing> {
    match s {
        "ES256" => Some(Signing::Es256),
        "ES384" => Some(Signing::Es384),
        "ES256K" => Some(Signing::Es256K),
        "EdDSA" => Some(Signing::EdDsa),
        _ => None,
    }
}

impl TryFrom<Vec<Jwk>> for Keyset {
    type Error = Error;

    fn try_from(keys: Vec<Jwk>) -> Result<Self> {
        if keys.is_empty() {
            return Err(Error::EmptyKeys);
        }
        let mut v = Vec::with_capacity(keys.len());
        let mut hs = HashSet::with_capacity(keys.len());
        for (i, key) in keys.into_iter().enumerate() {
            if let Some(kid) = key.prm.kid.clone() {
                if hs.contains(&kid) {
                    return Err(Error::DuplicateKid(kid));
                }
                hs.insert(kid);

                // Validate that the key has private material and is a supported type.
                match &key.key {
                    Key::Ec(ec) => {
                        if ec.d.is_none() {
                            return Err(Error::PublicKey);
                        }
                        if alg_for_key(&key.key).is_none() {
                            return Err(Error::UnsupportedKey);
                        }
                    }
                    Key::Okp(okp) => {
                        if okp.d.is_none() {
                            return Err(Error::PublicKey);
                        }
                        if alg_for_key(&key.key).is_none() {
                            return Err(Error::UnsupportedKey);
                        }
                    }
                    _ => return Err(Error::UnsupportedKey),
                }

                v.push(key);
            } else {
                return Err(Error::EmptyKid(i));
            }
        }
        Ok(Self(v))
    }
}
