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
#![cfg_attr(target_arch = "wasm32", allow(unused))]
use bon::Builder;
use bytes::Bytes;
use http::StatusCode;
use jacquard_common::error::BoxError;
use jacquard_common::types::did::Did;
use jacquard_common::types::did_doc::{DidDocument, Service};
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{AtprotoStr, Handle};
use jacquard_common::types::uri::Uri;
use jacquard_common::types::value::{AtDataError, Data};
use jacquard_common::{CowStr, IntoStatic, smol_str};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::marker::Sync;
use std::str::FromStr;
use url::Url;

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
    pub fn parse<'b>(&'b self) -> Result<DidDocument<'b>> {
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
                Err(IdentityError::missing_pds_endpoint())
            }
        } else {
            Err(IdentityError::http_status(self.status))
        }
    }

    /// Parse and validate that the DID in the document matches the requested DID if present.
    ///
    /// On mismatch, returns an error that contains the owned document for inspection.
    pub fn parse_validated<'b>(&'b self) -> Result<DidDocument<'b>> {
        let doc = self.parse()?;
        if let Some(expected) = &self.requested {
            if doc.id.as_str() != expected.as_str() {
                return Err(IdentityError::doc_id_mismatch(
                    expected.clone(),
                    doc.clone().into_static(),
                ));
            }
        }
        Ok(doc)
    }

    /// Parse as owned DidDocument<'static>
    pub fn into_owned(self) -> Result<DidDocument<'static>> {
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
                Err(IdentityError::missing_pds_endpoint())
            }
        } else {
            Err(IdentityError::http_status(self.status))
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
        let mut handle_order = vec![];
        #[cfg(not(target_family = "wasm"))]
        handle_order.push(HandleStep::DnsTxt);
        handle_order.push(HandleStep::HttpsWellKnown);
        handle_order.push(HandleStep::PdsResolveHandle);

        Self::new()
            .plc_source(PlcSource::default())
            .handle_order(handle_order)
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

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait IdentityResolver {
    /// Access options for validation decisions in default methods
    fn options(&self) -> &ResolverOptions;

    /// Resolve handle
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_handle(&self, handle: &Handle<'_>) -> impl Future<Output = Result<Did<'static>>>
    where
        Self: Sync;

    /// Resolve handle
    #[cfg(target_arch = "wasm32")]
    fn resolve_handle(&self, handle: &Handle<'_>) -> impl Future<Output = Result<Did<'static>>>;

    /// Resolve DID document
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_did_doc(&self, did: &Did<'_>) -> impl Future<Output = Result<DidDocResponse>>
    where
        Self: Sync;

    /// Resolve DID document
    #[cfg(target_arch = "wasm32")]
    fn resolve_did_doc(&self, did: &Did<'_>) -> impl Future<Output = Result<DidDocResponse>>;

    /// Resolve DID doc from an identifier
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_ident(
        &self,
        actor: &AtIdentifier<'_>,
    ) -> impl Future<Output = Result<DidDocResponse>>
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
    #[cfg(target_arch = "wasm32")]
    fn resolve_ident(
        &self,
        actor: &AtIdentifier<'_>,
    ) -> impl Future<Output = Result<DidDocResponse>> {
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
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_ident_owned(
        &self,
        actor: &AtIdentifier<'_>,
    ) -> impl Future<Output = Result<DidDocument<'static>>>
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

    /// Resolve DID doc from an identifier
    #[cfg(target_arch = "wasm32")]
    fn resolve_ident_owned(
        &self,
        actor: &AtIdentifier<'_>,
    ) -> impl Future<Output = Result<DidDocument<'static>>> {
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
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_did_doc_owned(
        &self,
        did: &Did<'_>,
    ) -> impl Future<Output = Result<DidDocument<'static>>>
    where
        Self: Sync,
    {
        async { self.resolve_did_doc(did).await?.into_owned() }
    }

    /// Resolve the DID document and return an owned version
    #[cfg(target_arch = "wasm32")]
    fn resolve_did_doc_owned(
        &self,
        did: &Did<'_>,
    ) -> impl Future<Output = Result<DidDocument<'static>>> {
        async { self.resolve_did_doc(did).await?.into_owned() }
    }

    /// Return the PDS url for a DID
    #[cfg(not(target_arch = "wasm32"))]
    fn pds_for_did(&self, did: &Did<'_>) -> impl Future<Output = Result<Url>>
    where
        Self: Sync,
    {
        async {
            let resp = self.resolve_did_doc(did).await?;
            let doc = resp.parse()?;
            // Default-on doc id equality check
            if self.options().validate_doc_id {
                if doc.id.as_str() != did.as_str() {
                    return Err(IdentityError::doc_id_mismatch(
                        did.clone().into_static(),
                        doc.clone().into_static(),
                    ));
                }
            }
            doc.pds_endpoint()
                .ok_or_else(|| IdentityError::missing_pds_endpoint())
        }
    }

    /// Return the PDS url for a DID
    #[cfg(target_arch = "wasm32")]
    fn pds_for_did(&self, did: &Did<'_>) -> impl Future<Output = Result<Url>> {
        async {
            let resp = self.resolve_did_doc(did).await?;
            let doc = resp.parse()?;
            // Default-on doc id equality check
            if self.options().validate_doc_id {
                if doc.id.as_str() != did.as_str() {
                    return Err(IdentityError::doc_id_mismatch(
                        did.clone().into_static(),
                        doc.clone().into_static(),
                    ));
                }
            }
            doc.pds_endpoint()
                .ok_or_else(|| IdentityError::missing_pds_endpoint())
        }
    }

    /// Return the DIS and PDS url for a handle
    #[cfg(not(target_arch = "wasm32"))]
    fn pds_for_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<Output = Result<(Did<'static>, Url)>>
    where
        Self: Sync,
    {
        async {
            let did = self.resolve_handle(handle).await?;
            let pds = self.pds_for_did(&did).await?;
            Ok((did, pds))
        }
    }

    /// Return the DIS and PDS url for a handle
    #[cfg(target_arch = "wasm32")]
    fn pds_for_handle(
        &self,
        handle: &Handle<'_>,
    ) -> impl Future<Output = Result<(Did<'static>, Url)>> {
        async {
            let did = self.resolve_handle(handle).await?;
            let pds = self.pds_for_did(&did).await?;
            Ok((did, pds))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: IdentityResolver + Sync> IdentityResolver for std::sync::Arc<T> {
    fn options(&self) -> &ResolverOptions {
        self.as_ref().options()
    }

    /// Resolve handle
    async fn resolve_handle(&self, handle: &Handle<'_>) -> Result<Did<'static>> {
        self.as_ref().resolve_handle(handle).await
    }

    /// Resolve DID document
    async fn resolve_did_doc(&self, did: &Did<'_>) -> Result<DidDocResponse> {
        self.as_ref().resolve_did_doc(did).await
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: IdentityResolver> IdentityResolver for std::sync::Arc<T> {
    fn options(&self) -> &ResolverOptions {
        self.as_ref().options()
    }

    /// Resolve handle
    async fn resolve_handle(&self, handle: &Handle<'_>) -> Result<Did<'static>> {
        self.as_ref().resolve_handle(handle).await
    }

    /// Resolve DID document
    async fn resolve_did_doc(&self, did: &Did<'_>) -> Result<DidDocResponse> {
        self.as_ref().resolve_did_doc(did).await
    }
}

/// Error type for identity resolution operations
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{kind}")]
pub struct IdentityError {
    #[diagnostic_source]
    kind: IdentityErrorKind,
    #[source]
    source: Option<BoxError>,
    #[help]
    help: Option<SmolStr>,
    context: Option<SmolStr>,
}

/// Error categories for identity resolution
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum IdentityErrorKind {
    /// Unsupported DID method
    #[error("unsupported DID method: {0}")]
    #[diagnostic(
        code(jacquard::identity::unsupported_method),
        help("supported DID methods: did:web, did:plc")
    )]
    UnsupportedDidMethod(SmolStr),

    /// Invalid well-known atproto-did content
    #[error("invalid well-known atproto-did content")]
    #[diagnostic(
        code(jacquard::identity::invalid_well_known),
        help("expected first non-empty line to be a DID")
    )]
    InvalidWellKnown,

    /// Missing PDS endpoint in DID document
    #[error("missing PDS endpoint in DID document")]
    #[diagnostic(
        code(jacquard::identity::missing_pds),
        help("ensure DID document contains AtprotoPersonalDataServer service")
    )]
    MissingPdsEndpoint,

    /// Transport-level error
    #[error("transport error")]
    #[diagnostic(
        code(jacquard::identity::transport),
        help("check network connectivity and TLS configuration")
    )]
    Transport,

    /// HTTP status error
    #[error("HTTP {0}")]
    #[diagnostic(
        code(jacquard::identity::http_status),
        help("verify well-known paths or PDS XRPC endpoints")
    )]
    HttpStatus(StatusCode),

    /// XRPC error
    #[error("XRPC error: {0}")]
    #[diagnostic(
        code(jacquard::identity::xrpc),
        help("enable PDS fallback or public resolver if needed")
    )]
    Xrpc(SmolStr),

    /// URL parse error
    #[error("URL parse error")]
    #[diagnostic(code(jacquard::identity::url))]
    Url,

    /// DNS resolution error
    #[cfg(all(feature = "dns", not(target_family = "wasm")))]
    #[error("DNS resolution error")]
    #[diagnostic(
        code(jacquard::identity::dns),
        help("check DNS configuration and connectivity")
    )]
    Dns,

    /// Serialization/deserialization error
    #[error("serialization error")]
    #[diagnostic(code(jacquard::identity::serialization))]
    Serialization,

    /// Invalid DID document
    #[error("invalid DID document: {0}")]
    #[diagnostic(
        code(jacquard::identity::invalid_doc),
        help("validate keys and services in DID document")
    )]
    InvalidDoc(SmolStr),

    /// DID document id mismatch - includes the fetched document for inspection
    #[error("DID document id mismatch")]
    #[diagnostic(
        code(jacquard::identity::doc_mismatch),
        help("document id differs from requested DID; do not trust this document")
    )]
    DocIdMismatch {
        expected: Did<'static>,
        doc: DidDocument<'static>,
    },
}

impl IdentityError {
    /// Create a new error with the given kind and optional source
    pub fn new(kind: IdentityErrorKind, source: Option<BoxError>) -> Self {
        Self {
            kind,
            source,
            help: None,
            context: None,
        }
    }

    /// Get the error kind
    pub fn kind(&self) -> &IdentityErrorKind {
        &self.kind
    }

    /// Get the source error if present
    pub fn source_err(&self) -> Option<&BoxError> {
        self.source.as_ref()
    }

    /// Get the context string if present
    pub fn context(&self) -> Option<&str> {
        self.context.as_ref().map(|s| s.as_str())
    }

    /// Add help text to this error
    pub fn with_help(mut self, help: impl Into<SmolStr>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add context to this error
    pub fn with_context(mut self, context: impl Into<SmolStr>) -> Self {
        self.context = Some(context.into());
        self
    }

    // Constructors for each kind

    /// Create an unsupported DID method error
    pub fn unsupported_did_method(method: impl Into<SmolStr>) -> Self {
        Self::new(IdentityErrorKind::UnsupportedDidMethod(method.into()), None)
    }

    /// Create an invalid well-known error
    pub fn invalid_well_known() -> Self {
        Self::new(IdentityErrorKind::InvalidWellKnown, None)
    }

    /// Create a missing PDS endpoint error
    pub fn missing_pds_endpoint() -> Self {
        Self::new(IdentityErrorKind::MissingPdsEndpoint, None)
    }

    /// Create a transport error
    pub fn transport(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(IdentityErrorKind::Transport, Some(Box::new(source)))
    }

    /// Create an HTTP status error
    pub fn http_status(status: StatusCode) -> Self {
        Self::new(IdentityErrorKind::HttpStatus(status), None)
    }

    /// Create an XRPC error
    pub fn xrpc(msg: impl Into<SmolStr>) -> Self {
        Self::new(IdentityErrorKind::Xrpc(msg.into()), None)
    }

    /// Create a URL parse error
    pub fn url(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(IdentityErrorKind::Url, Some(Box::new(source)))
    }

    /// Create a DNS error
    #[cfg(all(feature = "dns", not(target_family = "wasm")))]
    pub fn dns(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(IdentityErrorKind::Dns, Some(Box::new(source)))
    }

    /// Create a serialization error
    pub fn serialization(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(IdentityErrorKind::Serialization, Some(Box::new(source)))
    }

    /// Create an invalid doc error
    pub fn invalid_doc(msg: impl Into<SmolStr>) -> Self {
        Self::new(IdentityErrorKind::InvalidDoc(msg.into()), None)
    }

    /// Create a doc id mismatch error
    pub fn doc_id_mismatch(expected: Did<'static>, doc: DidDocument<'static>) -> Self {
        Self::new(IdentityErrorKind::DocIdMismatch { expected, doc }, None)
    }
}

/// Result type for identity operations
pub type Result<T> = std::result::Result<T, IdentityError>;

// ============================================================================
// Conversions from external errors
// ============================================================================

// #[allow(deprecated)]
// impl From<jacquard_common::error::TransportError> for IdentityError {
//     fn from(e: jacquard_common::error::TransportError) -> Self {
//         Self::transport(e).with_context("transport-level error during identity resolution")
//     }
// }

impl From<url::ParseError> for IdentityError {
    fn from(e: url::ParseError) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(IdentityErrorKind::Url, Some(Box::new(e))).with_context(msg)
    }
}

// Identity resolution errors -> ClientError
impl From<IdentityError> for jacquard_common::error::ClientError {
    fn from(e: IdentityError) -> Self {
        Self::identity_resolution(e)
    }
}

#[cfg(all(feature = "dns", not(target_family = "wasm")))]
impl From<hickory_resolver::error::ResolveError> for IdentityError {
    fn from(e: hickory_resolver::error::ResolveError) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(IdentityErrorKind::Dns, Some(Box::new(e)))
            .with_context(msg)
            .with_help("check DNS configuration and network connectivity")
    }
}

impl From<serde_json::Error> for IdentityError {
    fn from(e: serde_json::Error) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(IdentityErrorKind::Serialization, Some(Box::new(e)))
            .with_context(msg)
            .with_help("ensure response is valid JSON")
    }
}

impl From<AtDataError> for IdentityError {
    fn from(e: AtDataError) -> Self {
        let msg = smol_str::format_smolstr!("{:?}", e);
        Self::new(IdentityErrorKind::Serialization, Some(Box::new(e)))
            .with_context(msg)
            .with_help("AT Protocol data validation failed")
    }
}

impl From<reqwest::Error> for IdentityError {
    fn from(e: reqwest::Error) -> Self {
        Self::transport(e).with_context("HTTP request failed during identity resolution")
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
            Err(e) => match e.kind() {
                IdentityErrorKind::DocIdMismatch { expected, doc } => {
                    assert_eq!(expected.as_str(), "did:plc:alice");
                    assert_eq!(doc.id.as_str(), "did:plc:bob");
                }
                _ => panic!("unexpected error kind: {:?}", e),
            },
            other => panic!("unexpected result: {:?}", other),
        }
    }
}
