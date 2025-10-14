//! Identity resolution: handle → DID and DID → document, with smart fallbacks.
//!
//! Fallback order (default):
//! - Handle → DID: DNS TXT (if `dns` feature) → HTTPS well-known → PDS XRPC
//!   `resolveHandle` (when `pds_fallback` is configured) → public API fallback → Slingshot `resolveHandle` (if configured).
//! - DID → Doc: did:web well-known → PLC/Slingshot HTTP → PDS XRPC `resolveDid` (when configured),
//!   then Slingshot mini‑doc (partial) if configured.
//!
//! Parsing returns a `DidDocResponse` so callers can borrow from the response buffer
//! and optionally validate the document `id` against the requested DID.

use std::collections::BTreeMap;
use std::marker::Sync;
use std::str::FromStr;

use bon::Builder;
use bytes::Bytes;
use http::StatusCode;
use jacquard_common::error::TransportError;
use jacquard_common::types::did::Did;
use jacquard_common::types::did_doc::{DidDocument, Service};
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{AtprotoStr, Handle};
use jacquard_common::types::uri::Uri;
use jacquard_common::types::value::{AtDataError, Data};
use jacquard_common::{CowStr, IntoStatic};
use miette::Diagnostic;
use thiserror::Error;
use url::Url;

/// Errors that can occur during identity resolution.
///
/// Note: when validating a fetched DID document against a requested DID, a
/// `DocIdMismatch` error is returned that includes the owned document so callers
/// can inspect it and decide how to proceed.
#[derive(Debug, Error, Diagnostic)]
#[allow(missing_docs)]
pub enum IdentityError {
    #[error("unsupported DID method: {0}")]
    #[diagnostic(
        code(jacquard_identity::unsupported_did_method),
        help("supported DID methods: did:web, did:plc")
    )]
    UnsupportedDidMethod(String),
    #[error("invalid well-known atproto-did content")]
    #[diagnostic(
        code(jacquard_identity::invalid_well_known),
        help("expected first non-empty line to be a DID")
    )]
    InvalidWellKnown,
    #[error("missing PDS endpoint in DID document")]
    #[diagnostic(code(jacquard_identity::missing_pds_endpoint))]
    MissingPdsEndpoint,
    #[error("HTTP error: {0}")]
    #[diagnostic(
        code(jacquard_identity::http),
        help("check network connectivity and TLS configuration")
    )]
    Http(#[from] TransportError),
    #[error("HTTP status {0}")]
    #[diagnostic(
        code(jacquard_identity::http_status),
        help("verify well-known paths or PDS XRPC endpoints")
    )]
    HttpStatus(StatusCode),
    #[error("XRPC error: {0}")]
    #[diagnostic(
        code(jacquard_identity::xrpc),
        help("enable PDS fallback or public resolver if needed")
    )]
    Xrpc(String),
    #[error("URL parse error: {0}")]
    #[diagnostic(code(jacquard_identity::url))]
    Url(#[from] url::ParseError),
    #[error("DNS error: {0}")]
    #[cfg(feature = "dns")]
    #[diagnostic(code(jacquard_identity::dns))]
    Dns(#[from] hickory_resolver::error::ResolveError),
    #[error("serialize/deserialize error: {0}")]
    #[diagnostic(code(jacquard_identity::serde))]
    Serde(#[from] serde_json::Error),
    #[error("invalid DID document: {0}")]
    #[diagnostic(
        code(jacquard_identity::invalid_doc),
        help("validate keys and services; ensure AtprotoPersonalDataServer service exists")
    )]
    InvalidDoc(String),
    #[error(transparent)]
    #[diagnostic(code(jacquard_identity::data))]
    Data(#[from] AtDataError),
    /// DID document id did not match requested DID; includes the fetched document
    #[error("DID doc id mismatch")]
    #[diagnostic(
        code(jacquard_identity::doc_id_mismatch),
        help("document id differs from requested DID; do not trust this document")
    )]
    DocIdMismatch {
        expected: Did<'static>,
        doc: DidDocument<'static>,
    },
}

/// Source to fetch PLC (did:plc) documents from.
///
/// - `PlcDirectory`: uses the public PLC directory (default `https://plc.directory/`).
/// - `Slingshot`: uses Slingshot which also exposes convenience endpoints such as
///   `com.atproto.identity.resolveHandle` and a "mini-doc"
///   endpoint (`com.bad-example.identity.resolveMiniDoc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlcSource {
    /// Use the public PLC directory
    PlcDirectory {
        /// Base URL for the PLC directory
        base: Url,
    },
    /// Use the slingshot mini-docs service
    Slingshot {
        /// Base URL for the Slingshot service
        base: Url,
    },
}

impl Default for PlcSource {
    fn default() -> Self {
        Self::PlcDirectory {
            base: Url::parse("https://plc.directory/").expect("valid url"),
        }
    }
}

impl PlcSource {
    /// Default Slingshot source (`https://slingshot.microcosm.blue`)
    pub fn slingshot_default() -> Self {
        PlcSource::Slingshot {
            base: Url::parse("https://slingshot.microcosm.blue").expect("valid url"),
        }
    }
}

/// DID Document fetch response for borrowed/owned parsing.
///
/// Carries the raw response bytes and the HTTP status, plus the requested DID
/// (if supplied) to enable validation. Use `parse()` to borrow from the buffer
/// or `parse_validated()` to also enforce that the doc `id` matches the
/// requested DID (returns a `DocIdMismatch` containing the fetched doc on
/// mismatch). Use `into_owned()` to parse into an owned document.
#[derive(Clone)]
pub struct DidDocResponse {
    #[allow(missing_docs)]
    pub buffer: Bytes,
    #[allow(missing_docs)]
    pub status: StatusCode,
    /// Optional DID we intended to resolve; used for validation helpers
    pub requested: Option<Did<'static>>,
}

impl DidDocResponse {
    /// Parse as borrowed DidDocument<'_>
    pub fn parse<'b>(&'b self) -> Result<DidDocument<'b>, IdentityError> {
        if self.status.is_success() {
            if let Ok(doc) = serde_json::from_slice::<DidDocument<'b>>(&self.buffer) {
                Ok(doc)
            } else if let Ok(mini_doc) = serde_json::from_slice::<MiniDoc<'b>>(&self.buffer) {
                Ok(DidDocument {
                    id: mini_doc.did,
                    also_known_as: Some(vec![CowStr::from(mini_doc.handle)]),
                    verification_method: None,
                    service: Some(vec![Service {
                        id: CowStr::new_static("#atproto_pds"),
                        r#type: CowStr::new_static("AtprotoPersonalDataServer"),
                        service_endpoint: Some(Data::String(AtprotoStr::Uri(Uri::Https(
                            Url::from_str(&mini_doc.pds).unwrap(),
                        )))),
                        extra_data: BTreeMap::new(),
                    }]),
                    extra_data: BTreeMap::new(),
                })
            } else {
                Err(IdentityError::MissingPdsEndpoint)
            }
        } else {
            Err(IdentityError::HttpStatus(self.status))
        }
    }

    /// Parse and validate that the DID in the document matches the requested DID if present.
    ///
    /// On mismatch, returns an error that contains the owned document for inspection.
    pub fn parse_validated<'b>(&'b self) -> Result<DidDocument<'b>, IdentityError> {
        let doc = self.parse()?;
        if let Some(expected) = &self.requested {
            if doc.id.as_str() != expected.as_str() {
                return Err(IdentityError::DocIdMismatch {
                    expected: expected.clone(),
                    doc: doc.clone().into_static(),
                });
            }
        }
        Ok(doc)
    }

    /// Parse as owned DidDocument<'static>
    pub fn into_owned(self) -> Result<DidDocument<'static>, IdentityError> {
        if self.status.is_success() {
            if let Ok(doc) = serde_json::from_slice::<DidDocument<'_>>(&self.buffer) {
                Ok(doc.into_static())
            } else if let Ok(mini_doc) = serde_json::from_slice::<MiniDoc<'_>>(&self.buffer) {
                Ok(DidDocument {
                    id: mini_doc.did,
                    also_known_as: Some(vec![CowStr::from(mini_doc.handle)]),
                    verification_method: None,
                    service: Some(vec![Service {
                        id: CowStr::new_static("#atproto_pds"),
                        r#type: CowStr::new_static("AtprotoPersonalDataServer"),
                        service_endpoint: Some(Data::String(AtprotoStr::Uri(Uri::Https(
                            Url::from_str(&mini_doc.pds).unwrap(),
                        )))),
                        extra_data: BTreeMap::new(),
                    }]),
                    extra_data: BTreeMap::new(),
                }
                .into_static())
            } else {
                Err(IdentityError::MissingPdsEndpoint)
            }
        } else {
            Err(IdentityError::HttpStatus(self.status))
        }
    }
}

/// Slingshot mini-doc data (subset of DID doc info)
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct MiniDoc<'a> {
    #[serde(borrow)]
    pub did: Did<'a>,
    #[serde(borrow)]
    pub handle: Handle<'a>,
    #[serde(borrow)]
    pub pds: CowStr<'a>,
    #[serde(borrow, rename = "signingKey", alias = "signing_key")]
    pub signing_key: CowStr<'a>,
}

/// Handle → DID fallback step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleStep {
    /// DNS TXT _atproto.\<handle\>
    DnsTxt,
    /// HTTPS GET https://\<handle\>/.well-known/atproto-did
    HttpsWellKnown,
    /// XRPC com.atproto.identity.resolveHandle against a provided PDS base
    PdsResolveHandle,
}

/// DID → Doc fallback step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DidStep {
    /// For did:web: fetch from the well-known location
    DidWebHttps,
    /// For did:plc: fetch from PLC source
    PlcHttp,
    /// If a PDS base is known, ask it for the DID doc
    PdsResolveDid,
}

/// Configurable resolver options.
///
/// - `plc_source`: where to fetch did:plc documents (PLC Directory or Slingshot).
/// - `pds_fallback`: optional base URL of a PDS for XRPC fallbacks (stateless
///   XRPC over reqwest; authentication can be layered as needed).
/// - `handle_order`/`did_order`: ordered strategies for resolution.
/// - `validate_doc_id`: if true (default), convenience helpers validate doc `id` against the requested DID,
///   returning `DocIdMismatch` with the fetched document on mismatch.
/// - `public_fallback_for_handle`: if true (default), attempt
///   `https://public.api.bsky.app/xrpc/com.atproto.identity.resolveHandle` as an unauth fallback.
///   There is no public fallback for DID documents; when `PdsResolveDid` is chosen and the PDS XRPC
///   client fails, the resolver falls back to Slingshot mini-doc (partial) if `PlcSource::Slingshot` is configured.
#[derive(Debug, Clone, Builder)]
#[builder(start_fn = new)]
pub struct ResolverOptions {
    /// PLC data source (directory or slingshot)
    pub plc_source: PlcSource,
    /// Optional PDS base to use for fallbacks
    pub pds_fallback: Option<Url>,
    /// Order of attempts for handle → DID resolution
    pub handle_order: Vec<HandleStep>,
    /// Order of attempts for DID → Doc resolution
    pub did_order: Vec<DidStep>,
    /// Validate that fetched DID document id matches the requested DID
    pub validate_doc_id: bool,
    /// Allow public unauthenticated fallback for resolveHandle via public.api.bsky.app
    pub public_fallback_for_handle: bool,
}

impl Default for ResolverOptions {
    fn default() -> Self {
        // By default, prefer DNS then HTTPS for handles, then PDS fallback
        // For DID documents, prefer method-native sources, then PDS fallback
        Self::new()
            .plc_source(PlcSource::default())
            .handle_order(vec![
                HandleStep::DnsTxt,
                HandleStep::HttpsWellKnown,
                HandleStep::PdsResolveHandle,
            ])
            .did_order(vec![
                DidStep::DidWebHttps,
                DidStep::PlcHttp,
                DidStep::PdsResolveDid,
            ])
            .validate_doc_id(true)
            .public_fallback_for_handle(true)
            .build()
    }
}

/// Trait for identity resolution, for pluggable implementations.
///
/// The provided `DefaultResolver` supports:
/// - DNS TXT (`_atproto.<handle>`) when compiled with the `dns` feature
/// - HTTPS well-known for handles and `did:web`
/// - PLC directory or Slingshot for `did:plc`
/// - Slingshot `resolveHandle` (unauthenticated) when configured as the PLC source
/// - PDS fallbacks via helpers that use stateless XRPC on top of reqwest

pub trait IdentityResolver {
    /// Access options for validation decisions in default methods
    fn options(&self) -> &ResolverOptions;

    /// Resolve handle
    fn resolve_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<Output = Result<Did<'static>, IdentityError>> + Send
    where
        Self: Sync;

    /// Resolve DID document
    fn resolve_did_doc(
        &self,
        did: &Did<'_>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>> + Send
    where
        Self: Sync;

    /// Resolve DID doc from an identifier
    fn resolve_ident(
        &self,
        actor: &AtIdentifier<'_>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>> + Send
    where
        Self: Sync,
    {
        async move {
            match actor {
                AtIdentifier::Did(did) => self.resolve_did_doc(&did).await,
                AtIdentifier::Handle(handle) => {
                    let did = self.resolve_handle(&handle).await?;
                    self.resolve_did_doc(&did).await
                }
            }
        }
    }

    /// Resolve DID doc from an identifier
    fn resolve_ident_owned(
        &self,
        actor: &AtIdentifier<'_>,
    ) -> impl Future<Output = Result<DidDocument<'static>, IdentityError>> + Send
    where
        Self: Sync,
    {
        async move {
            match actor {
                AtIdentifier::Did(did) => self.resolve_did_doc_owned(&did).await,
                AtIdentifier::Handle(handle) => {
                    let did = self.resolve_handle(&handle).await?;
                    self.resolve_did_doc_owned(&did).await
                }
            }
        }
    }

    /// Resolve the DID document and return an owned version
    fn resolve_did_doc_owned(
        &self,
        did: &Did<'_>,
    ) -> impl Future<Output = Result<DidDocument<'static>, IdentityError>> + Send
    where
        Self: Sync,
    {
        async { self.resolve_did_doc(did).await?.into_owned() }
    }
    /// Return the PDS url for a DID
    fn pds_for_did(&self, did: &Did<'_>) -> impl Future<Output = Result<Url, IdentityError>> + Send
    where
        Self: Sync,
    {
        async {
            let resp = self.resolve_did_doc(did).await?;
            let doc = resp.parse()?;
            // Default-on doc id equality check
            if self.options().validate_doc_id {
                if doc.id.as_str() != did.as_str() {
                    return Err(IdentityError::DocIdMismatch {
                        expected: did.clone().into_static(),
                        doc: doc.clone().into_static(),
                    });
                }
            }
            doc.pds_endpoint().ok_or(IdentityError::MissingPdsEndpoint)
        }
    }
    /// Return the DIS and PDS url for a handle
    fn pds_for_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<Output = Result<(Did<'static>, Url), IdentityError>> + Send
    where
        Self: Sync,
    {
        async {
            let did = self.resolve_handle(handle).await?;
            let pds = self.pds_for_did(&did).await?;
            Ok((did, pds))
        }
    }
}

impl<T: IdentityResolver + Sync> IdentityResolver for std::sync::Arc<T> {
    fn options(&self) -> &ResolverOptions {
        self.as_ref().options()
    }

    /// Resolve handle
    async fn resolve_handle(&self, handle: &Handle<'_>) -> Result<Did<'static>, IdentityError> {
        self.as_ref().resolve_handle(handle).await
    }

    /// Resolve DID document
    async fn resolve_did_doc(&self, did: &Did<'_>) -> Result<DidDocResponse, IdentityError> {
        self.as_ref().resolve_did_doc(did).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_validated_ok() {
        let buf = Bytes::from_static(br#"{"id":"did:plc:alice"}"#);
        let requested = Did::new_owned("did:plc:alice").unwrap();
        let resp = DidDocResponse {
            buffer: buf,
            status: StatusCode::OK,
            requested: Some(requested),
        };
        let _doc = resp.parse_validated().expect("valid");
    }

    #[test]
    fn parse_validated_mismatch() {
        let buf = Bytes::from_static(br#"{"id":"did:plc:bob"}"#);
        let requested = Did::new_owned("did:plc:alice").unwrap();
        let resp = DidDocResponse {
            buffer: buf,
            status: StatusCode::OK,
            requested: Some(requested),
        };
        match resp.parse_validated() {
            Err(IdentityError::DocIdMismatch { expected, doc }) => {
                assert_eq!(expected.as_str(), "did:plc:alice");
                assert_eq!(doc.id.as_str(), "did:plc:bob");
            }
            other => panic!("unexpected result: {:?}", other),
        }
    }
}
