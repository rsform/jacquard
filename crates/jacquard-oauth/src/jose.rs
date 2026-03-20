/// JWS (JSON Web Signature) header types.
pub mod jws;
/// JWT (JSON Web Token) claims types.
pub mod jwt;
/// Signed JWT creation for supported algorithms (ES256, ES384, ES256K, EdDSA).
pub mod signing;

use serde::{Deserialize, Serialize};

/// A JOSE header, covering the supported JWS formats.
///
/// Serialized as an untagged enum so the wire format matches the relevant JOSE spec directly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Header<'a> {
    /// A JWS compact-serialization header.
    #[serde(borrow)]
    Jws(jws::Header<'a>),
}

