//! Multikey decoding and optional conversions.
//!
//! This module provides a small `PublicKey` wrapper that can decode a
//! Multikey `publicKeyMultibase` string into raw bytes plus a codec
//! (`KeyCodec`). Feature‑gated helpers convert to popular Rust crypto
//! public‑key types (ed25519_dalek, k256, p256).
//! Example: decode an ed25519 multibase key
//! ```
//! use jacquard_common::types::crypto::{PublicKey, KeyCodec};
//! // ed25519 key: multicodec varint 0xED + 32 raw bytes, base58btc encoded
//! let mut key = [0u8; 32];
//! let s = {
//!   fn enc(mut x: u64) -> Vec<u8> { let mut v=Vec::new(); while x>=0x80{v.push(((x as u8)&0x7F)|0x80); x >>= 7;} v.push(x as u8); v }
//!   let mut buf = enc(0xED); buf.extend_from_slice(&key); multibase::encode(multibase::Base::Base58Btc, buf)
//! };
//! let pk = PublicKey::decode(&s).unwrap();
//! assert!(matches!(pk.codec, KeyCodec::Ed25519));
//! assert_eq!(pk.bytes.as_ref(), &key);

use crate::IntoStatic;
use std::borrow::Cow;

/// Known multicodec key codecs for Multikey public keys
///

/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCodec {
    /// Ed25519
    Ed25519,
    /// Secp256k1
    Secp256k1,
    /// P256
    P256,
    /// Unknown codec
    Unknown(u64),
}

/// Public key decoded from a Multikey `publicKeyMultibase` string
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey<'a> {
    /// Codec used to encode the public key
    pub codec: KeyCodec,
    /// Bytes of the public key
    pub bytes: Cow<'a, [u8]>,
}

#[cfg(feature = "crypto")]
fn code_of(codec: KeyCodec) -> u64 {
    match codec {
        KeyCodec::Ed25519 => 0xED,
        KeyCodec::Secp256k1 => 0xE7,
        KeyCodec::P256 => 0x1200,
        KeyCodec::Unknown(c) => c,
    }
}

/// Errors from decoding or converting Multikey values
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic, PartialEq, Eq)]
pub enum CryptoError {
    #[error("failed to decode multibase")]
    /// Multibase decode errror
    MultibaseDecode,
    #[error("failed to decode multicodec varint")]
    /// Multicodec decode error
    MulticodecDecode,
    #[error("unsupported key codec: {0}")]
    /// Unsupported key codec error
    UnsupportedCodec(u64),
    #[error("invalid key length: expected {expected}, got {got}")]
    /// Invalid key length error
    InvalidLength {
        /// Expected length of the key
        expected: usize,
        /// Actual length of the key
        got: usize,
    },
    #[error("invalid key format")]
    /// Invalid key format error
    InvalidFormat,
    #[error("conversion error: {0}")]
    /// Conversion error
    Conversion(String),
}

impl<'a> PublicKey<'a> {
    /// Decode a Multikey public key from a multibase-encoded string
    pub fn decode(multibase_str: &'a str) -> Result<PublicKey<'static>, CryptoError> {
        let (_base, data) =
            multibase::decode(multibase_str).map_err(|_| CryptoError::MultibaseDecode)?;
        let (code, offset) = decode_uvarint(&data).ok_or(CryptoError::MulticodecDecode)?;
        let bytes = &data[offset..];
        let codec = match code {
            0xED => KeyCodec::Ed25519,   // ed25519-pub
            0xE7 => KeyCodec::Secp256k1, // secp256k1-pub
            0x1200 => KeyCodec::P256,    // p256-pub
            other => KeyCodec::Unknown(other),
        };
        // Minimal validation
        match codec {
            KeyCodec::Ed25519 => {
                if bytes.len() != 32 {
                    return Err(CryptoError::InvalidLength {
                        expected: 32,
                        got: bytes.len(),
                    });
                }
            }
            KeyCodec::Secp256k1 | KeyCodec::P256 => {
                if !(bytes.len() == 33 || bytes.len() == 65) {
                    return Err(CryptoError::InvalidLength {
                        expected: 33,
                        got: bytes.len(),
                    });
                }
                // 0x02/0x03 compressed, 0x04 uncompressed
                let first = *bytes.first().ok_or(CryptoError::InvalidFormat)?;
                if first != 0x02 && first != 0x03 && first != 0x04 {
                    return Err(CryptoError::InvalidFormat);
                }
            }
            KeyCodec::Unknown(code) => return Err(CryptoError::UnsupportedCodec(code)),
        }
        Ok(PublicKey {
            codec,
            bytes: Cow::Owned(bytes.to_vec()),
        })
    }

    // decode_owned provided on PublicKey<'static>

    /// Convert to ed25519_dalek verifying key (feature crypto-ed25519)
    #[cfg(feature = "crypto-ed25519")]
    pub fn to_ed25519(&self) -> Result<ed25519_dalek::VerifyingKey, CryptoError> {
        if self.codec != KeyCodec::Ed25519 {
            return Err(CryptoError::UnsupportedCodec(code_of(self.codec)));
        }
        ed25519_dalek::VerifyingKey::from_bytes(self.bytes.as_ref().try_into().map_err(|_| {
            CryptoError::InvalidLength {
                expected: 32,
                got: self.bytes.len(),
            }
        })?)
        .map_err(|e| CryptoError::Conversion(e.to_string()))
    }

    /// Convert to k256 public key (feature crypto-k256)
    #[cfg(feature = "crypto-k256")]
    pub fn to_k256(&self) -> Result<k256::PublicKey, CryptoError> {
        if self.codec != KeyCodec::Secp256k1 {
            return Err(CryptoError::UnsupportedCodec(code_of(self.codec)));
        }
        k256::PublicKey::from_sec1_bytes(self.bytes.as_ref())
            .map_err(|e| CryptoError::Conversion(e.to_string()))
    }

    /// Convert to p256 public key (feature crypto-p256)
    #[cfg(feature = "crypto-p256")]
    pub fn to_p256(&self) -> Result<p256::PublicKey, CryptoError> {
        if self.codec != KeyCodec::P256 {
            return Err(CryptoError::UnsupportedCodec(code_of(self.codec)));
        }
        p256::PublicKey::from_sec1_bytes(self.bytes.as_ref())
            .map_err(|e| CryptoError::Conversion(e.to_string()))
    }
}

impl PublicKey<'static> {
    /// Decode from an owned string-like value
    pub fn decode_owned(s: impl AsRef<str>) -> Result<PublicKey<'static>, CryptoError> {
        PublicKey::decode(s.as_ref())
    }
}

impl IntoStatic for PublicKey<'_> {
    type Output = PublicKey<'static>;
    fn into_static(self) -> Self::Output {
        match self.bytes {
            Cow::Borrowed(b) => PublicKey {
                codec: self.codec,
                bytes: Cow::Owned(b.to_vec()),
            },
            Cow::Owned(b) => PublicKey {
                codec: self.codec,
                bytes: Cow::Owned(b),
            },
        }
    }
}

fn decode_uvarint(data: &[u8]) -> Option<(u64, usize)> {
    let mut x: u64 = 0;
    let mut s: u32 = 0;
    for (i, b) in data.iter().copied().enumerate() {
        if b < 0x80 {
            if i > 9 || (i == 9 && b > 1) {
                return None;
            }
            return Some((x | ((b as u64) << s), i + 1));
        }
        x |= ((b & 0x7F) as u64) << s;
        s += 7;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use multibase;

    fn encode_uvarint(mut x: u64) -> Vec<u8> {
        let mut out = Vec::new();
        while x >= 0x80 {
            out.push(((x as u8) & 0x7F) | 0x80);
            x >>= 7;
        }
        out.push(x as u8);
        out
    }

    fn multikey(code: u64, key: &[u8]) -> String {
        let mut buf = encode_uvarint(code);
        buf.extend_from_slice(key);
        multibase::encode(multibase::Base::Base58Btc, buf)
    }

    #[test]
    fn decode_ed25519() {
        let key = [0u8; 32];
        let s = multikey(0xED, &key);
        let pk = PublicKey::decode(&s).expect("decode");
        assert_eq!(pk.codec, KeyCodec::Ed25519);
        assert_eq!(pk.bytes.as_ref(), &key);
    }

    #[test]
    fn decode_k1_compressed() {
        let mut key = [0u8; 33];
        key[0] = 0x02; // compressed y-bit
        let s = multikey(0xE7, &key);
        let pk = PublicKey::decode(&s).expect("decode");
        assert_eq!(pk.codec, KeyCodec::Secp256k1);
        assert_eq!(pk.bytes.as_ref(), &key);
    }

    #[test]
    fn decode_p256_uncompressed() {
        let mut key = [0u8; 65];
        key[0] = 0x04; // uncompressed
        let s = multikey(0x1200, &key);
        let pk = PublicKey::decode(&s).expect("decode");
        assert_eq!(pk.codec, KeyCodec::P256);
        assert_eq!(pk.bytes.as_ref(), &key);
    }

    #[cfg(feature = "crypto-ed25519")]
    #[test]
    fn ed25519_conversion_ok() {
        use core::convert::TryFrom;
        use ed25519_dalek::{SecretKey, SigningKey, VerifyingKey};
        // Build a deterministic signing key from a fixed secret
        let secret = SecretKey::try_from(&[7u8; 32][..]).expect("secret");
        let sk = SigningKey::from_bytes(&secret);
        let vk: VerifyingKey = sk.verifying_key();
        let bytes = vk.to_bytes();
        // Encode multikey: varint(0xED) + key bytes, base58btc
        let mut buf = super::tests::encode_uvarint(0xED);
        buf.extend_from_slice(&bytes);
        let s = multibase::encode(multibase::Base::Base58Btc, buf);
        let pk = PublicKey::decode(&s).expect("decode");
        assert!(matches!(pk.codec, KeyCodec::Ed25519));
        let vk2 = pk.to_ed25519().expect("to ed25519");
        assert_eq!(vk.as_bytes(), vk2.as_bytes());
    }

    #[cfg(feature = "crypto-k256")]
    #[test]
    fn k256_unsupported_on_ed25519_codec() {
        // Use a valid-looking ed25519 key, attempt k256 conversion → UnsupportedCodec
        let key = [1u8; 32];
        let s = super::tests::multikey(0xED, &key);
        let pk = PublicKey::decode(&s).expect("decode");
        let err = pk.to_k256().unwrap_err();
        assert!(matches!(err, CryptoError::UnsupportedCodec(_)));
    }

    #[cfg(feature = "crypto-p256")]
    #[test]
    fn p256_unsupported_on_ed25519_codec() {
        // Use a valid-looking ed25519 key, attempt p256 conversion → UnsupportedCodec
        let key = [2u8; 32];
        let s = super::tests::multikey(0xED, &key);
        let pk = PublicKey::decode(&s).expect("decode");
        let err = pk.to_p256().unwrap_err();
        assert!(matches!(err, CryptoError::UnsupportedCodec(_)));
    }
}
