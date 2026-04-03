/// JWS (JSON Web Signature) header types.
pub mod jws;
/// JWT (JSON Web Token) claims types.
pub mod jwt;
/// Signed JWT creation for supported algorithms (ES256, ES384, ES256K, EdDSA).
pub mod signing;

use jacquard_common::bos::{BosStr, DefaultStr};
use serde::{Deserialize, Serialize};

/// A JOSE header, covering the supported JWS formats.
///
/// Serialized as an untagged enum so the wire format matches the relevant JOSE spec directly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[serde(bound(deserialize = "S: serde::Deserialize<'de> + BosStr"))]
pub enum Header<S: BosStr = DefaultStr> {
    /// A JWS compact-serialization header.
    Jws(jws::Header<S>),
}
