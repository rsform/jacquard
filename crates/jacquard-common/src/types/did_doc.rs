use crate::deps::fluent_uri::Uri;
use crate::types::crypto::{CryptoError, PublicKey};
use crate::types::string::{AtprotoStr, Did, Handle};
use crate::types::value::Data;
use crate::{Bos, DefaultStr, IntoStatic};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use bon::Builder;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// DID Document representation with borrowed data where possible.
///
/// Only the most commonly used fields are modeled explicitly. All other fields
/// are captured in `extra_data` for forward compatibility, using the same
/// pattern as lexicon structs.
///
/// Example
/// ```
/// use jacquard_common::types::did_doc::DidDocument;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let json = r##"{
///   "id": "did:plc:alice",
///   "alsoKnownAs": ["at://alice.example"],
///   "service": [{
///     "id": "#pds",
///     "type": "AtprotoPersonalDataServer",
///     "serviceEndpoint": "https://pds.example"
///   }],
///   "verificationMethod": [{
///     "id": "#k",
///     "type": "Multikey",
///     "publicKeyMultibase": "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
///   }]
/// }"##;
/// let doc: DidDocument = serde_json::from_str(json)?;
/// assert_eq!(doc.id.as_str(), "did:plc:alice");
/// assert!(doc.pds_endpoint().is_some());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Builder)]
#[builder(start_fn = new)]
#[serde(rename_all = "camelCase")]
#[serde(bound(
    serialize = "S: Serialize + Bos<str> + AsRef<str>",
    deserialize = "S: Deserialize<'de> + Bos<str> + AsRef<str>"
))]
pub struct DidDocument<S: Bos<str> + AsRef<str> = DefaultStr> {
    /// required prelude
    #[serde(rename = "@context")]
    #[serde(default = "default_context")]
    pub context: Vec<SmolStr>,

    /// Document identifier (e.g., `did:plc:...` or `did:web:...`)
    pub id: Did<S>,

    /// Alternate identifiers for the subject, such as at://\<handle\>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub also_known_as: Option<Vec<S>>,

    /// Verification methods (keys) for this DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_method: Option<Vec<VerificationMethod<S>>>,

    /// Services associated with this DID (e.g., AtprotoPersonalDataServer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<Vec<Service<S>>>,
    // Forward‑compatible capture of unmodeled fields
    #[serde(flatten)]
    pub extra_data: BTreeMap<SmolStr, Data<S>>,
}

/// Default context fields for DID documents
pub fn default_context() -> Vec<SmolStr> {
    vec![
        SmolStr::new_static("https://www.w3.org/ns/did/v1"),
        SmolStr::new_static("https://w3id.org/security/multikey/v1"),
        SmolStr::new_static("https://w3id.org/security/suites/secp256k1-2019/v1"),
    ]
}

impl<S> crate::IntoStatic for DidDocument<S>
where
    S: Bos<str> + AsRef<str> + crate::IntoStatic,
    <S as IntoStatic>::Output: AsRef<str>,
    <S as IntoStatic>::Output: Bos<str>,
{
    type Output = DidDocument<<S as crate::IntoStatic>::Output>;
    fn into_static(self) -> Self::Output {
        DidDocument {
            context: default_context(),
            id: self.id.into_static(),
            also_known_as: self.also_known_as.into_static(),
            verification_method: self.verification_method.into_static(),
            service: self.service.into_static(),
            // TODO: re-enable extra data fields
            extra_data: self.extra_data.into_static(),
        }
    }
}

impl<S> DidDocument<S>
where
    S: Bos<str> + AsRef<str> + Clone,
{
    /// Extract validated handles from `alsoKnownAs` entries like `at://\<handle\>`.
    pub fn handles(&self) -> Vec<Handle<&str>> {
        self.also_known_as
            .as_ref()
            .map(|v| {
                v.iter()
                    .filter_map(|h| {
                        let s = h.as_ref().strip_prefix("at://").unwrap_or(h.as_ref());
                        Handle::new(s).ok()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract the first Multikey `publicKeyMultibase` value from verification methods.
    pub fn atproto_multikey(&self) -> Option<S> {
        self.verification_method.as_ref().and_then(|methods| {
            methods.iter().find_map(|m| {
                if m.r#type.as_ref() == "Multikey" {
                    m.public_key_multibase.as_ref().map(|k| k.clone())
                } else {
                    None
                }
            })
        })
    }

    /// Extract the AtprotoPersonalDataServer service endpoint as a `fluent_uri::Uri<&str>`.
    /// Accepts endpoint as string or object (string preferred).
    pub fn pds_endpoint(&self) -> Option<Uri<&str>> {
        self.service.as_ref().and_then(|services| {
            services.iter().find_map(|s| {
                if s.r#type.as_ref() == "AtprotoPersonalDataServer" {
                    match &s.service_endpoint {
                        Some(Data::String(AtprotoStr::Uri(u))) => Uri::parse(u.as_ref()).ok(),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        })
    }

    /// Decode the atproto Multikey (first occurrence) into a typed public key.
    pub fn atproto_public_key(&self) -> Result<Option<PublicKey<'static>>, CryptoError> {
        if let Some(multibase) = self.atproto_multikey() {
            let pk = PublicKey::decode(multibase.as_ref())?;
            Ok(Some(pk))
        } else {
            Ok(None)
        }
    }
}

/// Verification method (key) entry in a DID Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Builder)]
#[builder(start_fn = new)]
#[serde(rename_all = "camelCase")]
#[serde(bound(
    serialize = "S: Serialize + Bos<str> + AsRef<str>",
    deserialize = "S: Deserialize<'de> + Bos<str> + AsRef<str>"
))]
pub struct VerificationMethod<S: Bos<str> + AsRef<str>> {
    /// Identifier for this key material within the document
    pub id: S,
    /// Key type (e.g., `Multikey`)
    #[serde(rename = "type")]
    pub r#type: S,
    /// Optional controller DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller: Option<S>,
    /// Multikey `publicKeyMultibase` (base58btc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key_multibase: Option<S>,
    // Forward‑compatible capture of unmodeled fields
    #[serde(flatten)]
    pub extra_data: BTreeMap<SmolStr, Data<S>>,
}

impl<S> crate::IntoStatic for VerificationMethod<S>
where
    S: Bos<str> + AsRef<str> + crate::IntoStatic,
    <S as IntoStatic>::Output: AsRef<str>,
    <S as IntoStatic>::Output: Bos<str>,
{
    type Output = VerificationMethod<<S as crate::IntoStatic>::Output>;
    fn into_static(self) -> Self::Output {
        VerificationMethod {
            id: self.id.into_static(),
            r#type: self.r#type.into_static(),
            controller: self.controller.into_static(),
            public_key_multibase: self.public_key_multibase.into_static(),
            // TODO: re-enable extra data fields
            extra_data: self.extra_data.into_static(),
        }
    }
}

/// Service entry in a DID Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Builder)]
#[builder(start_fn = new)]
#[serde(rename_all = "camelCase")]
#[serde(bound(
    serialize = "S: Serialize + Bos<str> + AsRef<str>",
    deserialize = "S: Deserialize<'de> + Bos<str> + AsRef<str>"
))]
pub struct Service<S: Bos<str> + AsRef<str>> {
    /// Service identifier
    pub id: S,
    /// Service type (e.g., `AtprotoPersonalDataServer`)
    #[serde(rename = "type")]
    pub r#type: S,
    /// currently atproto expects this to be a url
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_endpoint: Option<Data<S>>,
    // Forward‑compatible capture of unmodeled fields
    #[serde(flatten)]
    pub extra_data: BTreeMap<SmolStr, Data<S>>,
}

impl<S> crate::IntoStatic for Service<S>
where
    S: Bos<str> + AsRef<str> + crate::IntoStatic,
    <S as IntoStatic>::Output: AsRef<str>,
    <S as IntoStatic>::Output: Bos<str>,
{
    type Output = Service<<S as crate::IntoStatic>::Output>;
    fn into_static(self) -> Self::Output {
        Service {
            id: self.id.into_static(),
            r#type: self.r#type.into_static(),
            service_endpoint: self.service_endpoint.into_static(),
            // TODO: re-enable extra data fields
            extra_data: self.extra_data.into_static(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use serde_json::json;

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
    fn public_key_decode() {
        let did = "did:plc:example";
        let mut k = [0u8; 32];
        k[0] = 7;
        let mk = multikey(0xED, &k);
        let doc_json = json!({
            "id": did,
            "verificationMethod": [
                {
                    "id": "#key-1",
                    "type": "Multikey",
                    "publicKeyMultibase": mk,
                }
            ]
        });
        let doc_string = serde_json::to_string(&doc_json).unwrap();
        let doc: DidDocument = serde_json::from_str(&doc_string).unwrap();
        let pk = doc.atproto_public_key().unwrap().expect("present");
        assert!(matches!(pk.codec, crate::types::crypto::KeyCodec::Ed25519));
        assert_eq!(pk.bytes.as_ref(), &k);
    }

    #[test]
    fn parse_sample_doc_and_helpers() {
        let raw = include_str!("test_did_doc.json");
        let doc: DidDocument = serde_json::from_str(raw).expect("parse doc");
        // id
        assert_eq!(doc.id.as_str(), "did:plc:yfvwmnlztr4dwkb7hwz55r2g");
        // pds endpoint
        let pds = doc.pds_endpoint().expect("pds endpoint");
        assert_eq!(pds.as_str(), "https://atproto.systems");
        // handle alias extraction
        let handles = doc.handles();
        assert!(handles.iter().any(|h| h.as_str() == "nonbinary.computer"));
        // multikey string present
        let mk = doc.atproto_multikey().expect("has multikey");
        assert!(AsRef::<str>::as_ref(&mk).starts_with('z'));
        // typed decode (may be ed25519, secp256k1, or p256 depending on multicodec)
        let _ = doc.atproto_public_key().expect("decode ok");
    }
}
