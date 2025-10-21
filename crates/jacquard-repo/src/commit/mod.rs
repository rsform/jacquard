//! Commit structures and signature verification for AT Protocol repositories.
//!
//! This module provides repository commit object handling with signature support.

pub mod firehose;
pub mod proof;
pub(crate) mod serde_bytes_helper;
use crate::error::{CommitError, Result};
use bytes::Bytes;
use cid::Cid as IpldCid;
use jacquard_common::IntoStatic;
use jacquard_common::types::crypto::PublicKey;
use jacquard_common::types::string::Did;
use jacquard_common::types::tid::Tid;
/// Repository commit object
///
/// This structure represents a signed commit in an AT Protocol repository.
/// Stored as a block in CAR files, identified by its CID.
///
/// **Version compatibility**: v2 and v3 commits differ only in how `prev` is
/// serialized (v2 uses it, v3 must include it even if null). This struct
/// handles both by always including `prev` in serialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Commit<'a> {
    /// Repository DID
    #[serde(borrow)]
    pub did: Did<'a>,

    /// Commit version (2 or 3)
    pub version: i64,

    /// MST root CID
    pub data: IpldCid,

    /// Revision TID
    pub rev: Tid,

    /// Previous commit CID (None for initial commit)
    pub prev: Option<IpldCid>,

    /// Signature bytes
    #[serde(with = "serde_bytes_helper")]
    pub sig: Bytes,
}

impl<'a> Commit<'a> {
    /// Create new unsigned commit (version = 3, sig empty)
    pub fn new_unsigned(did: Did<'a>, data: IpldCid, rev: Tid, prev: Option<IpldCid>) -> Self {
        Self {
            did,
            version: 3,
            data,
            rev,
            prev,
            sig: Bytes::new(),
        }
    }

    /// Sign this commit with a key
    pub fn sign(mut self, key: &impl SigningKey) -> Result<Self> {
        let unsigned = self.unsigned_bytes()?;
        self.sig = key.sign_bytes(&unsigned)?;
        Ok(self)
    }

    /// Get the repository DID
    pub fn did(&self) -> &Did<'a> {
        &self.did
    }

    /// Get the MST root CID
    pub fn data(&self) -> &IpldCid {
        &self.data
    }

    /// Get the revision TID
    pub fn rev(&self) -> &Tid {
        &self.rev
    }

    /// Get the previous commit CID
    pub fn prev(&self) -> Option<&IpldCid> {
        self.prev.as_ref()
    }

    /// Get the signature bytes
    pub fn sig(&self) -> &Bytes {
        &self.sig
    }

    /// Get unsigned commit bytes (for signing/verification)
    pub(super) fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        // Serialize without signature field
        let mut unsigned = self.clone();
        unsigned.sig = Bytes::new();
        serde_ipld_dagcbor::to_vec(&unsigned)
            .map_err(|e| crate::error::CommitError::Serialization(Box::new(e)).into())
    }

    /// Serialize to DAG-CBOR
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        serde_ipld_dagcbor::to_vec(self).map_err(|e| CommitError::Serialization(Box::new(e)).into())
    }

    /// Deserialize from DAG-CBOR
    pub fn from_cbor(data: &'a [u8]) -> Result<Self> {
        serde_ipld_dagcbor::from_slice(data)
            .map_err(|e| CommitError::Serialization(Box::new(e)).into())
    }

    /// Compute CID of this commit
    pub fn to_cid(&self) -> Result<IpldCid> {
        let cbor = self.to_cbor()?;
        crate::mst::util::compute_cid(&cbor)
    }

    /// Verify signature against a public key from a DID document.
    ///
    /// The key type is inferred from the PublicKey codec.
    pub fn verify(&self, pubkey: &PublicKey) -> std::result::Result<(), CommitError> {
        let unsigned = self
            .unsigned_bytes()
            .map_err(|e| CommitError::Serialization(e.into()))?;
        let signature = self.sig();

        use jacquard_common::types::crypto::KeyCodec;
        match pubkey.codec {
            KeyCodec::Ed25519 => {
                let vk = pubkey
                    .to_ed25519()
                    .map_err(|e| CommitError::InvalidKey(e.to_string()))?;
                let sig = ed25519_dalek::Signature::from_slice(signature.as_ref())
                    .map_err(|e| CommitError::InvalidSignature(e.to_string()))?;
                vk.verify_strict(&unsigned, &sig)
                    .map_err(|_| CommitError::SignatureVerificationFailed)?;
            }
            KeyCodec::Secp256k1 => {
                use k256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
                let vk = pubkey
                    .to_k256()
                    .map_err(|e| CommitError::InvalidKey(e.to_string()))?;
                let verifying_key = VerifyingKey::from(&vk);
                let sig = Signature::from_slice(signature.as_ref())
                    .map_err(|e| CommitError::InvalidSignature(e.to_string()))?;
                verifying_key
                    .verify(&unsigned, &sig)
                    .map_err(|_| CommitError::SignatureVerificationFailed)?;
            }
            KeyCodec::P256 => {
                use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
                let vk = pubkey
                    .to_p256()
                    .map_err(|e| CommitError::InvalidKey(e.to_string()))?;
                let verifying_key = VerifyingKey::from(&vk);
                let sig = Signature::from_slice(signature.as_ref())
                    .map_err(|e| CommitError::InvalidSignature(e.to_string()))?;
                verifying_key
                    .verify(&unsigned, &sig)
                    .map_err(|_| CommitError::SignatureVerificationFailed)?;
            }
            KeyCodec::Unknown(code) => {
                return Err(CommitError::UnsupportedKeyType(code));
            }
        }

        Ok(())
    }
}

impl IntoStatic for Commit<'_> {
    type Output = Commit<'static>;

    fn into_static(self) -> Self::Output {
        Commit {
            did: self.did.into_static(),
            version: self.version,
            data: self.data,
            rev: self.rev,
            prev: self.prev,
            sig: self.sig,
        }
    }
}

/// Trait for signing keys.
///
/// Implemented for ed25519_dalek::SigningKey, k256::ecdsa::SigningKey, and p256::ecdsa::SigningKey.
pub trait SigningKey {
    /// Sign the given data and return signature as Bytes
    fn sign_bytes(&self, data: &[u8]) -> Result<Bytes>;

    /// Get the public key bytes
    fn public_key(&self) -> Vec<u8>;
}

// Ed25519 implementation
impl SigningKey for ed25519_dalek::SigningKey {
    fn sign_bytes(&self, data: &[u8]) -> Result<Bytes> {
        use ed25519_dalek::Signer;
        let sig = Signer::sign(self, data);
        Ok(Bytes::copy_from_slice(&sig.to_bytes()))
    }

    fn public_key(&self) -> Vec<u8> {
        self.verifying_key().to_bytes().to_vec()
    }
}

// K-256 (secp256k1) implementation
impl SigningKey for k256::ecdsa::SigningKey {
    fn sign_bytes(&self, data: &[u8]) -> Result<Bytes> {
        use k256::ecdsa::signature::Signer;
        let sig: k256::ecdsa::Signature = Signer::sign(self, data);
        Ok(Bytes::copy_from_slice(&sig.to_bytes()))
    }

    fn public_key(&self) -> Vec<u8> {
        self.verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec()
    }
}

// P-256 implementation
impl SigningKey for p256::ecdsa::SigningKey {
    fn sign_bytes(&self, data: &[u8]) -> Result<Bytes> {
        use p256::ecdsa::signature::Signer;
        let sig: p256::ecdsa::Signature = Signer::sign(self, data);
        Ok(Bytes::copy_from_slice(&sig.to_bytes()))
    }

    fn public_key(&self) -> Vec<u8> {
        self.verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec()
    }
}
