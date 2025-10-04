use crate::types::crypto::{CryptoError, PublicKey};
use crate::types::string::{Did, Handle};
use crate::types::value::Data;
use crate::{CowStr, IntoStatic};
use bon::Builder;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use url::Url;

/// DID Document representation with borrowed data where possible.
///
/// Only the most commonly used fields are modeled explicitly. All other fields
/// are captured in `extra_data` for forward compatibility, using the same
/// pattern as lexicon structs.
///
/// Example
/// ```ignore
/// use jacquard_common::types::did_doc::DidDocument;
/// use serde_json::json;
/// let doc: DidDocument<'_> = serde_json::from_value(json!({
///   "id": "did:plc:alice",
///   "alsoKnownAs": ["at://alice.example"],
///   "service": [{"id":"#pds","type":"AtprotoPersonalDataServer","serviceEndpoint":"https://pds.example"}],
///   "verificationMethod":[{"id":"#k","type":"Multikey","publicKeyMultibase":"z6Mki..."}]
/// })).unwrap();
/// assert_eq!(doc.id.as_str(), "did:plc:alice");
/// assert!(doc.pds_endpoint().is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Builder)]
#[builder(start_fn = new)]
#[serde(rename_all = "camelCase")]
pub struct DidDocument<'a> {
    /// Document identifier (e.g., `did:plc:...` or `did:web:...`)
    #[serde(borrow)]
    pub id: Did<'a>,

    /// Alternate identifiers for the subject, such as at://<handle>
    #[serde(borrow)]
    pub also_known_as: Option<Vec<CowStr<'a>>>,

    /// Verification methods (keys) for this DID
    #[serde(borrow)]
    pub verification_method: Option<Vec<VerificationMethod<'a>>>,

    /// Services associated with this DID (e.g., AtprotoPersonalDataServer)
    #[serde(borrow)]
    pub service: Option<Vec<Service<'a>>>,

    /// Forward‑compatible capture of unmodeled fields
    #[serde(flatten)]
    pub extra_data: BTreeMap<SmolStr, Data<'a>>,
}

impl crate::IntoStatic for DidDocument<'_> {
    type Output = DidDocument<'static>;
    fn into_static(self) -> Self::Output {
        DidDocument {
            id: self.id.into_static(),
            also_known_as: self.also_known_as.into_static(),
            verification_method: self.verification_method.into_static(),
            service: self.service.into_static(),
            extra_data: self.extra_data.into_static(),
        }
    }
}

impl<'a> DidDocument<'a> {
    /// Extract validated handles from `alsoKnownAs` entries like `at://<handle>`.
    pub fn handles(&self) -> Vec<Handle<'static>> {
        self.also_known_as
            .as_ref()
            .map(|v| {
                v.iter()
                    .filter_map(|s| s.strip_prefix("at://"))
                    .filter_map(|h| Handle::new(h).ok())
                    .map(|h| h.into_static())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract the first Multikey `publicKeyMultibase` value from verification methods.
    pub fn atproto_multikey(&self) -> Option<CowStr<'static>> {
        self.verification_method.as_ref().and_then(|methods| {
            methods.iter().find_map(|m| {
                if m.r#type.as_ref() == "Multikey" {
                    m.public_key_multibase
                        .as_ref()
                        .map(|k| k.clone().into_static())
                } else {
                    None
                }
            })
        })
    }

    /// Extract the AtprotoPersonalDataServer service endpoint as a `Url`.
    /// Accepts endpoint as string or object (string preferred).
    pub fn pds_endpoint(&self) -> Option<Url> {
        self.service.as_ref().and_then(|services| {
            services.iter().find_map(|s| {
                if s.r#type.as_ref() == "AtprotoPersonalDataServer" {
                    match &s.service_endpoint {
                        Some(Data::String(strv)) => Url::parse(strv.as_ref()).ok(),
                        Some(Data::Object(obj)) => {
                            // Some documents may include structured endpoints; try common fields
                            if let Some(Data::String(urlv)) = obj.0.get("url") {
                                Url::parse(urlv.as_ref()).ok()
                            } else {
                                None
                            }
                        }
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
            let pk = PublicKey::decode(&multibase)?;
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
pub struct VerificationMethod<'a> {
    /// Identifier for this key material within the document
    #[serde(borrow)]
    pub id: CowStr<'a>,
    /// Key type (e.g., `Multikey`)
    #[serde(borrow, rename = "type")]
    pub r#type: CowStr<'a>,
    /// Optional controller DID
    #[serde(borrow)]
    pub controller: Option<CowStr<'a>>,
    /// Multikey `publicKeyMultibase` (base58btc)
    #[serde(borrow)]
    pub public_key_multibase: Option<CowStr<'a>>,

    /// Forward‑compatible capture of unmodeled fields
    #[serde(flatten)]
    pub extra_data: BTreeMap<SmolStr, Data<'a>>,
}

impl crate::IntoStatic for VerificationMethod<'_> {
    type Output = VerificationMethod<'static>;
    fn into_static(self) -> Self::Output {
        VerificationMethod {
            id: self.id.into_static(),
            r#type: self.r#type.into_static(),
            controller: self.controller.into_static(),
            public_key_multibase: self.public_key_multibase.into_static(),
            extra_data: self.extra_data.into_static(),
        }
    }
}

/// Service entry in a DID Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Builder)]
#[builder(start_fn = new)]
#[serde(rename_all = "camelCase")]
pub struct Service<'a> {
    /// Service identifier
    #[serde(borrow)]
    pub id: CowStr<'a>,
    /// Service type (e.g., `AtprotoPersonalDataServer`)
    #[serde(borrow, rename = "type")]
    pub r#type: CowStr<'a>,
    /// String or object; we preserve as Data
    #[serde(borrow)]
    pub service_endpoint: Option<Data<'a>>,

    /// Forward‑compatible capture of unmodeled fields
    #[serde(flatten)]
    pub extra_data: BTreeMap<SmolStr, Data<'a>>,
}

impl crate::IntoStatic for Service<'_> {
    type Output = Service<'static>;
    fn into_static(self) -> Self::Output {
        Service {
            id: self.id.into_static(),
            r#type: self.r#type.into_static(),
            service_endpoint: self.service_endpoint.into_static(),
            extra_data: self.extra_data.into_static(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let doc: DidDocument<'_> = serde_json::from_str(&doc_string).unwrap();
        let pk = doc.atproto_public_key().unwrap().expect("present");
        assert!(matches!(pk.codec, crate::types::crypto::KeyCodec::Ed25519));
        assert_eq!(pk.bytes.as_ref(), &k);
    }

    #[test]
    fn parse_sample_doc_and_helpers() {
        let raw = include_str!("test_did_doc.json");
        let doc: DidDocument<'_> = serde_json::from_str(raw).expect("parse doc");
        // id
        assert_eq!(doc.id.as_str(), "did:plc:yfvwmnlztr4dwkb7hwz55r2g");
        // pds endpoint
        let pds = doc.pds_endpoint().expect("pds endpoint");
        assert_eq!(pds.as_str(), "https://atproto.systems/");
        // handle alias extraction
        let handles = doc.handles();
        assert!(handles.iter().any(|h| h.as_str() == "nonbinary.computer"));
        // multikey string present
        let mk = doc.atproto_multikey().expect("has multikey");
        assert!(mk.as_ref().starts_with('z'));
        // typed decode (may be ed25519, secp256k1, or p256 depending on multicodec)
        let _ = doc.atproto_public_key().expect("decode ok");
    }
}
