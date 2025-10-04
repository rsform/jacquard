//! Identity resolution: handle → DID and DID → document, with smart fallbacks.
//!
//! Fallback order (default):
//! - Handle → DID: DNS TXT (if `dns` feature) → HTTPS well-known → embedded XRPC
//!   `resolveHandle` → public API fallback → Slingshot `resolveHandle` (if configured).
//! - DID → Doc: did:web well-known → PLC/slingshot HTTP → embedded XRPC `resolveDid`,
//!   then Slingshot mini‑doc (partial) if configured.
//!
//! Parsing returns a `DidDocResponse` so callers can borrow from the response buffer
//! and optionally validate the document `id` against the requested DID.

use crate::CowStr;
use crate::client::AuthenticatedClient;
use bon::Builder;
use bytes::Bytes;
use jacquard_common::IntoStatic;
use miette::Diagnostic;
use percent_encoding::percent_decode_str;
use reqwest::StatusCode;
use thiserror::Error;
use url::{ParseError, Url};

use crate::api::com_atproto::identity::{resolve_did, resolve_handle::ResolveHandle};
use crate::types::did_doc::DidDocument;
use crate::types::ident::AtIdentifier;
use crate::types::string::{Did, Handle};
use crate::types::value::AtDataError;

#[cfg(feature = "dns")]
use hickory_resolver::{TokioAsyncResolver, config::ResolverConfig};

/// Errors that can occur during identity resolution.
///
/// Note: when validating a fetched DID document against a requested DID, a
/// `DocIdMismatch` error is returned that includes the owned document so callers
/// can inspect it and decide how to proceed.
#[derive(Debug, Error, Diagnostic)]
#[allow(missing_docs)]
pub enum IdentityError {
    #[error("unsupported DID method: {0}")]
    UnsupportedDidMethod(String),
    #[error("invalid well-known atproto-did content")]
    InvalidWellKnown,
    #[error("missing PDS endpoint in DID document")]
    MissingPdsEndpoint,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("HTTP status {0}")]
    HttpStatus(StatusCode),
    #[error("XRPC error: {0}")]
    Xrpc(String),
    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),
    #[error("DNS error: {0}")]
    #[cfg(feature = "dns")]
    Dns(#[from] hickory_resolver::error::ResolveError),
    #[error("serialize/deserialize error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid DID document: {0}")]
    InvalidDoc(String),
    #[error(transparent)]
    Data(#[from] AtDataError),
    /// DID document id did not match requested DID; includes the fetched document
    #[error("DID doc id mismatch")]
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
    buffer: Bytes,
    status: StatusCode,
    /// Optional DID we intended to resolve; used for validation helpers
    requested: Option<Did<'static>>,
}

impl DidDocResponse {
    /// Parse as borrowed DidDocument<'_>
    pub fn parse<'b>(&'b self) -> Result<DidDocument<'b>, IdentityError> {
        if self.status.is_success() {
            serde_json::from_slice::<DidDocument<'b>>(&self.buffer).map_err(IdentityError::from)
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
            serde_json::from_slice::<DidDocument<'_>>(&self.buffer)
                .map(|d| d.into_static())
                .map_err(IdentityError::from)
        } else {
            Err(IdentityError::HttpStatus(self.status))
        }
    }
}

/// Handle → DID fallback step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleStep {
    /// DNS TXT _atproto.<handle>
    DnsTxt,
    /// HTTPS GET https://<handle>/.well-known/atproto-did
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
/// - `pds_fallback`: optional base URL of a PDS for XRPC fallbacks (auth-aware
///   paths available via helpers that take an `XrpcClient`).
/// - `handle_order`/`did_order`: ordered strategies for resolution.
/// - `validate_doc_id`: if true (default), convenience helpers validate doc `id` against the requested DID,
///   returning `DocIdMismatch` with the fetched document on mismatch.
/// - `public_fallback_for_handle`: if true (default), attempt
///   `https://public.api.bsky.app/xrpc/com.atproto.identity.resolveHandle` as an unauth fallback.
///   There is no public fallback for DID documents; when `PdsResolveDid` is chosen and the embedded XRPC
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
/// - Auth-aware PDS fallbacks via helpers that accept an `XrpcClient`
#[async_trait::async_trait]
pub trait IdentityResolver {
    /// Access options for validation decisions in default methods
    fn options(&self) -> &ResolverOptions;

    /// Resolve handle
    async fn resolve_handle(&self, handle: &Handle<'_>) -> Result<Did<'static>, IdentityError>;

    /// Resolve DID document
    async fn resolve_did_doc(&self, did: &Did<'_>) -> Result<DidDocResponse, IdentityError>;
    async fn resolve_did_doc_owned(
        &self,
        did: &Did<'_>,
    ) -> Result<DidDocument<'static>, IdentityError> {
        self.resolve_did_doc(did).await?.into_owned()
    }
    async fn pds_for_did(&self, did: &Did<'_>) -> Result<Url, IdentityError> {
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
    async fn pds_for_handle(
        &self,
        handle: &Handle<'_>,
    ) -> Result<(Did<'static>, Url), IdentityError> {
        let did = self.resolve_handle(handle).await?;
        let pds = self.pds_for_did(&did).await?;
        Ok((did, pds))
    }
}

/// Default resolver implementation with configurable fallback order.
///
/// Behavior highlights:
/// - Handle resolution tries DNS TXT (if enabled via `dns` feature), then HTTPS
///   well-known, then Slingshot's unauthenticated `resolveHandle` when
///   `PlcSource::Slingshot` is configured.
/// - DID resolution tries did:web well-known for `did:web`, and the configured
///   PLC base (PLC directory or Slingshot) for `did:plc`.
/// - PDS-authenticated fallbacks (e.g., `resolveHandle`, `resolveDid` on a PDS)
///   are available via helper methods that accept a user-provided `XrpcClient`.
///
/// Example
/// ```ignore
/// use jacquard::identity::resolver::{DefaultResolver, ResolverOptions};
/// use jacquard::client::{AuthenticatedClient, XrpcClient};
/// use jacquard::types::string::Handle;
/// use jacquard::CowStr;
///
/// // Build an auth-capable XRPC client (without a session it behaves like public/unauth)
/// let http = reqwest::Client::new();
/// let xrpc = AuthenticatedClient::new(http.clone(), CowStr::from("https://bsky.social"));
/// let resolver = DefaultResolver::new(http, xrpc, ResolverOptions::default());
///
/// // Resolve a handle to a DID
/// let did = tokio_test::block_on(async { resolver.resolve_handle(&Handle::new("bad-example.com").unwrap()).await }).unwrap();
/// ```
pub struct DefaultResolver<C: crate::client::XrpcClient + Send + Sync> {
    http: reqwest::Client,
    xrpc: C,
    opts: ResolverOptions,
    #[cfg(feature = "dns")]
    dns: Option<TokioAsyncResolver>,
}

impl<C: crate::client::XrpcClient + Send + Sync> DefaultResolver<C> {
    pub fn new(http: reqwest::Client, xrpc: C, opts: ResolverOptions) -> Self {
        Self {
            http,
            xrpc,
            opts,
            #[cfg(feature = "dns")]
            dns: None,
        }
    }

    #[cfg(feature = "dns")]
    pub fn with_system_dns(mut self) -> Self {
        self.dns = Some(TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            Default::default(),
        ));
        self
    }

    /// Set PLC source (PLC directory or Slingshot)
    ///
    /// Example
    /// ```ignore
    /// use jacquard::identity::resolver::{DefaultResolver, ResolverOptions, PlcSource};
    /// let http = reqwest::Client::new();
    /// let xrpc = jacquard::client::AuthenticatedClient::new(http.clone(), jacquard::CowStr::from("https://public.api.bsky.app"));
    /// let resolver = DefaultResolver::new(http, xrpc, ResolverOptions::default())
    ///     .with_plc_source(PlcSource::slingshot_default());
    /// ```
    pub fn with_plc_source(mut self, source: PlcSource) -> Self {
        self.opts.plc_source = source;
        self
    }

    /// Enable/disable public unauthenticated fallback for resolveHandle
    ///
    /// Example
    /// ```ignore
    /// # use jacquard::identity::resolver::{DefaultResolver, ResolverOptions};
    /// # let http = reqwest::Client::new();
    /// # let xrpc = jacquard::client::AuthenticatedClient::new(http.clone(), jacquard::CowStr::from("https://public.api.bsky.app"));
    /// let resolver = DefaultResolver::new(http, xrpc, ResolverOptions::default())
    ///     .with_public_fallback_for_handle(true);
    /// ```
    pub fn with_public_fallback_for_handle(mut self, enable: bool) -> Self {
        self.opts.public_fallback_for_handle = enable;
        self
    }

    /// Enable/disable doc id validation
    ///
    /// Example
    /// ```ignore
    /// # use jacquard::identity::resolver::{DefaultResolver, ResolverOptions};
    /// # let http = reqwest::Client::new();
    /// # let xrpc = jacquard::client::AuthenticatedClient::new(http.clone(), jacquard::CowStr::from("https://public.api.bsky.app"));
    /// let resolver = DefaultResolver::new(http, xrpc, ResolverOptions::default())
    ///     .with_validate_doc_id(true);
    /// ```
    pub fn with_validate_doc_id(mut self, enable: bool) -> Self {
        self.opts.validate_doc_id = enable;
        self
    }

    /// Construct the well-known HTTPS URL for a `did:web` DID.
    ///
    /// - `did:web:example.com` → `https://example.com/.well-known/did.json`
    /// - `did:web:example.com:user:alice` → `https://example.com/user/alice/did.json`
    fn did_web_url(&self, did: &Did<'_>) -> Result<Url, IdentityError> {
        // did:web:example.com[:path:segments]
        let s = did.as_str();
        let rest = s
            .strip_prefix("did:web:")
            .ok_or_else(|| IdentityError::UnsupportedDidMethod(s.to_string()))?;
        let mut parts = rest.split(':');
        let host = parts
            .next()
            .ok_or_else(|| IdentityError::UnsupportedDidMethod(s.to_string()))?;
        let mut url = Url::parse(&format!("https://{host}/")).map_err(IdentityError::Url)?;
        let path: Vec<&str> = parts.collect();
        if path.is_empty() {
            url.set_path(".well-known/did.json");
        } else {
            // Append path segments and did.json
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| IdentityError::Url(ParseError::SetHostOnCannotBeABaseUrl))?;
            for seg in path {
                // Minimally percent-decode each segment per spec guidance
                let decoded = percent_decode_str(seg).decode_utf8_lossy();
                segments.push(&decoded);
            }
            segments.push("did.json");
            // drop segments
        }
        Ok(url)
    }

    #[cfg(test)]
    fn test_did_web_url_raw(&self, s: &str) -> String {
        let did = Did::new(s).unwrap();
        self.did_web_url(&did).unwrap().to_string()
    }

    async fn get_json_bytes(&self, url: Url) -> Result<(Bytes, StatusCode), IdentityError> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let buf = resp.bytes().await?;
        Ok((buf, status))
    }

    async fn get_text(&self, url: Url) -> Result<String, IdentityError> {
        let resp = self.http.get(url).send().await?;
        if resp.status() == StatusCode::OK {
            Ok(resp.text().await?)
        } else {
            Err(IdentityError::Http(resp.error_for_status().unwrap_err()))
        }
    }

    #[cfg(feature = "dns")]
    async fn dns_txt(&self, name: &str) -> Result<Vec<String>, IdentityError> {
        let Some(dns) = &self.dns else {
            return Ok(vec![]);
        };
        let fqdn = format!("_atproto.{name}.");
        let response = dns.txt_lookup(fqdn).await?;
        let mut out = Vec::new();
        for txt in response.iter() {
            for data in txt.txt_data().iter() {
                out.push(String::from_utf8_lossy(data).to_string());
            }
        }
        Ok(out)
    }

    fn parse_atproto_did_body(body: &str) -> Result<Did<'static>, IdentityError> {
        let line = body
            .lines()
            .find(|l| !l.trim().is_empty())
            .ok_or(IdentityError::InvalidWellKnown)?;
        let did = Did::new(line.trim()).map_err(|_| IdentityError::InvalidWellKnown)?;
        Ok(did.into_static())
    }
}

impl<C: crate::client::XrpcClient + Send + Sync> DefaultResolver<C> {
    /// Resolve handle to DID via a PDS XRPC client (auth-aware path)
    pub async fn resolve_handle_via_pds(
        &self,
        handle: &Handle<'_>,
    ) -> Result<Did<'static>, IdentityError> {
        let req = ResolveHandle::new().handle((*handle).clone()).build();
        let resp = self
            .xrpc
            .send(req)
            .await
            .map_err(|e| IdentityError::Xrpc(e.to_string()))?;
        let out = resp
            .into_output()
            .map_err(|e| IdentityError::Xrpc(e.to_string()))?;
        Did::new_owned(out.did.as_str())
            .map(|d| d.into_static())
            .map_err(|_| IdentityError::InvalidWellKnown)
    }

    /// Fetch DID document via PDS resolveDid (returns owned DidDocument)
    pub async fn fetch_did_doc_via_pds_owned(
        &self,
        did: &Did<'_>,
    ) -> Result<DidDocument<'static>, IdentityError> {
        let req = resolve_did::ResolveDid::new().did(did.clone()).build();
        let resp = self
            .xrpc
            .send(req)
            .await
            .map_err(|e| IdentityError::Xrpc(e.to_string()))?;
        let out = resp
            .into_output()
            .map_err(|e| IdentityError::Xrpc(e.to_string()))?;
        let doc_json = serde_json::to_value(&out.did_doc)?;
        let s = serde_json::to_string(&doc_json)?;
        let doc_borrowed: DidDocument<'_> = serde_json::from_str(&s)?;
        Ok(doc_borrowed.into_static())
    }

    /// Fetch a minimal DID document via a Slingshot mini-doc endpoint, if your PlcSource uses Slingshot.
    /// Returns the raw response wrapper for borrowed parsing and validation.
    pub async fn fetch_mini_doc_via_slingshot(
        &self,
        did: &Did<'_>,
    ) -> Result<DidDocResponse, IdentityError> {
        let base = match &self.opts.plc_source {
            PlcSource::Slingshot { base } => base.clone(),
            _ => {
                return Err(IdentityError::UnsupportedDidMethod(
                    "mini-doc requires Slingshot source".into(),
                ));
            }
        };
        let mut url = base;
        url.set_path("/xrpc/com.bad-example.identity.resolveMiniDoc");
        if let Ok(qs) =
            serde_html_form::to_string(&resolve_did::ResolveDid::new().did(did.clone()).build())
        {
            url.set_query(Some(&qs));
        }
        let (buf, status) = self.get_json_bytes(url).await?;
        Ok(DidDocResponse {
            buffer: buf,
            status,
            requested: Some(did.clone().into_static()),
        })
    }
}

#[async_trait::async_trait]
impl<C: crate::client::XrpcClient + Send + Sync> IdentityResolver for DefaultResolver<C> {
    fn options(&self) -> &ResolverOptions {
        &self.opts
    }
    async fn resolve_handle(&self, handle: &Handle<'_>) -> Result<Did<'static>, IdentityError> {
        let host = handle.as_str();
        for step in &self.opts.handle_order {
            match step {
                HandleStep::DnsTxt => {
                    #[cfg(feature = "dns")]
                    {
                        if let Ok(txts) = self.dns_txt(host).await {
                            for txt in txts {
                                if let Some(did_str) = txt.strip_prefix("did=") {
                                    if let Ok(did) = Did::new(did_str) {
                                        return Ok(did.into_static());
                                    }
                                }
                            }
                        }
                    }
                }
                HandleStep::HttpsWellKnown => {
                    let url = Url::parse(&format!("https://{host}/.well-known/atproto-did"))?;
                    if let Ok(text) = self.get_text(url).await {
                        if let Ok(did) = Self::parse_atproto_did_body(&text) {
                            return Ok(did);
                        }
                    }
                }
                HandleStep::PdsResolveHandle => {
                    // Prefer embedded XRPC client
                    if let Ok(did) = self.resolve_handle_via_pds(handle).await {
                        return Ok(did);
                    }
                    // Public unauth fallback
                    if self.opts.public_fallback_for_handle {
                        if let Ok(mut url) = Url::parse("https://public.api.bsky.app") {
                            url.set_path("/xrpc/com.atproto.identity.resolveHandle");
                            if let Ok(qs) = serde_html_form::to_string(
                                &ResolveHandle::new().handle((*handle).clone()).build(),
                            ) {
                                url.set_query(Some(&qs));
                            } else {
                                continue;
                            }
                            if let Ok((buf, status)) = self.get_json_bytes(url).await {
                                if status.is_success() {
                                    if let Ok(val) =
                                        serde_json::from_slice::<serde_json::Value>(&buf)
                                    {
                                        if let Some(did_str) =
                                            val.get("did").and_then(|v| v.as_str())
                                        {
                                            if let Ok(did) = Did::new_owned(did_str) {
                                                return Ok(did.into_static());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Non-auth path: if PlcSource is Slingshot, use its resolveHandle endpoint.
                    if let PlcSource::Slingshot { base } = &self.opts.plc_source {
                        let mut url = base.clone();
                        url.set_path("/xrpc/com.atproto.identity.resolveHandle");
                        if let Ok(qs) = serde_html_form::to_string(
                            &ResolveHandle::new().handle((*handle).clone()).build(),
                        ) {
                            url.set_query(Some(&qs));
                        } else {
                            continue;
                        }
                        if let Ok((buf, status)) = self.get_json_bytes(url).await {
                            if status.is_success() {
                                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&buf) {
                                    if let Some(did_str) = val.get("did").and_then(|v| v.as_str()) {
                                        if let Ok(did) = Did::new_owned(did_str) {
                                            return Ok(did.into_static());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(IdentityError::InvalidWellKnown)
    }

    async fn resolve_did_doc(&self, did: &Did<'_>) -> Result<DidDocResponse, IdentityError> {
        let s = did.as_str();
        for step in &self.opts.did_order {
            match step {
                DidStep::DidWebHttps if s.starts_with("did:web:") => {
                    let url = self.did_web_url(did)?;
                    if let Ok((buf, status)) = self.get_json_bytes(url).await {
                        return Ok(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                    }
                }
                DidStep::PlcHttp if s.starts_with("did:plc:") => {
                    let url = match &self.opts.plc_source {
                        PlcSource::PlcDirectory { base } => base.join(did.as_str())?,
                        PlcSource::Slingshot { base } => base.join(did.as_str())?,
                    };
                    if let Ok((buf, status)) = self.get_json_bytes(url).await {
                        return Ok(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                    }
                }
                DidStep::PdsResolveDid => {
                    // Try embedded XRPC client for full DID doc
                    if let Ok(doc) = self.fetch_did_doc_via_pds_owned(did).await {
                        let buf = serde_json::to_vec(&doc).unwrap_or_default();
                        return Ok(DidDocResponse {
                            buffer: Bytes::from(buf),
                            status: StatusCode::OK,
                            requested: Some(did.clone().into_static()),
                        });
                    }
                    // Fallback: if Slingshot configured, return mini-doc response (partial doc)
                    if let PlcSource::Slingshot { base } = &self.opts.plc_source {
                        let url = self.slingshot_mini_doc_url(base, did.as_str())?;
                        let (buf, status) = self.get_json_bytes(url).await?;
                        return Ok(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                    }
                }
                _ => {}
            }
        }
        Err(IdentityError::UnsupportedDidMethod(s.to_string()))
    }
}

/// Warnings produced during identity checks that are not fatal
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityWarning {
    /// The DID doc did not contain the expected handle alias under alsoKnownAs
    HandleAliasMismatch { expected: Handle<'static> },
}

impl<C: crate::client::XrpcClient + Send + Sync> DefaultResolver<C> {
    /// Resolve a handle to its DID, fetch the DID document, and return doc plus any warnings.
    /// This applies the default equality check on the document id (error with doc if mismatch).
    pub async fn resolve_handle_and_doc(
        &self,
        handle: &Handle<'_>,
    ) -> Result<(Did<'static>, DidDocResponse, Vec<IdentityWarning>), IdentityError> {
        let did = self.resolve_handle(handle).await?;
        let resp = self.resolve_did_doc(&did).await?;
        let resp_for_parse = resp.clone();
        let doc_borrowed = resp_for_parse.parse()?;
        if self.opts.validate_doc_id && doc_borrowed.id.as_str() != did.as_str() {
            return Err(IdentityError::DocIdMismatch {
                expected: did.clone().into_static(),
                doc: doc_borrowed.clone().into_static(),
            });
        }
        let mut warnings = Vec::new();
        // Check handle alias presence (soft warning)
        let expected_alias = format!("at://{}", handle.as_str());
        let has_alias = doc_borrowed
            .also_known_as
            .as_ref()
            .map(|v| v.iter().any(|s| s.as_ref() == expected_alias))
            .unwrap_or(false);
        if !has_alias {
            warnings.push(IdentityWarning::HandleAliasMismatch {
                expected: handle.clone().into_static(),
            });
        }
        Ok((did, resp, warnings))
    }

    /// Build Slingshot mini-doc URL for an identifier (handle or DID)
    fn slingshot_mini_doc_url(&self, base: &Url, identifier: &str) -> Result<Url, IdentityError> {
        let mut url = base.clone();
        url.set_path("/xrpc/com.bad-example.identity.resolveMiniDoc");
        url.set_query(Some(&format!(
            "identifier={}",
            urlencoding::Encoded::new(identifier)
        )));
        Ok(url)
    }

    /// Fetch a minimal DID document via Slingshot's mini-doc endpoint using a generic at-identifier
    pub async fn fetch_mini_doc_via_slingshot_identifier(
        &self,
        identifier: &AtIdentifier<'_>,
    ) -> Result<MiniDocResponse, IdentityError> {
        let base = match &self.opts.plc_source {
            PlcSource::Slingshot { base } => base.clone(),
            _ => {
                return Err(IdentityError::UnsupportedDidMethod(
                    "mini-doc requires Slingshot source".into(),
                ));
            }
        };
        let url = self.slingshot_mini_doc_url(&base, identifier.as_str())?;
        let (buf, status) = self.get_json_bytes(url).await?;
        Ok(MiniDocResponse {
            buffer: buf,
            status,
        })
    }
}

/// Slingshot mini-doc JSON response wrapper
#[derive(Clone)]
pub struct MiniDocResponse {
    buffer: Bytes,
    status: StatusCode,
}

impl MiniDocResponse {
    /// Parse borrowed MiniDoc
    pub fn parse<'b>(&'b self) -> Result<MiniDoc<'b>, IdentityError> {
        if self.status.is_success() {
            serde_json::from_slice::<MiniDoc<'b>>(&self.buffer).map_err(IdentityError::from)
        } else {
            Err(IdentityError::HttpStatus(self.status))
        }
    }
}

/// Slingshot mini-doc data (subset of DID doc info)
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiniDoc<'a> {
    #[serde(borrow)]
    pub did: Did<'a>,
    #[serde(borrow)]
    pub handle: Handle<'a>,
    #[serde(borrow)]
    pub pds: crate::CowStr<'a>,
    #[serde(borrow, rename = "signingKey", alias = "signing_key")]
    pub signing_key: crate::CowStr<'a>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_web_urls() {
        let r = DefaultResolver::new(
            reqwest::Client::new(),
            TestXrpc::new(),
            ResolverOptions::default(),
        );
        assert_eq!(
            r.test_did_web_url_raw("did:web:example.com"),
            "https://example.com/.well-known/did.json"
        );
        assert_eq!(
            r.test_did_web_url_raw("did:web:example.com:user:alice"),
            "https://example.com/user/alice/did.json"
        );
    }

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

    #[test]
    fn slingshot_mini_doc_url_build() {
        let r = DefaultResolver::new(
            reqwest::Client::new(),
            TestXrpc::new(),
            ResolverOptions::default(),
        );
        let base = Url::parse("https://slingshot.microcosm.blue").unwrap();
        let url = r.slingshot_mini_doc_url(&base, "bad-example.com").unwrap();
        assert_eq!(
            url.as_str(),
            "https://slingshot.microcosm.blue/xrpc/com.bad-example.identity.resolveMiniDoc?identifier=bad-example.com"
        );
    }

    #[test]
    fn slingshot_mini_doc_parse_success() {
        let buf = Bytes::from_static(
            br#"{
  "did": "did:plc:hdhoaan3xa3jiuq4fg4mefid",
  "handle": "bad-example.com",
  "pds": "https://porcini.us-east.host.bsky.network",
  "signing_key": "zQ3shpq1g134o7HGDb86CtQFxnHqzx5pZWknrVX2Waum3fF6j"
}"#,
        );
        let resp = MiniDocResponse {
            buffer: buf,
            status: StatusCode::OK,
        };
        let doc = resp.parse().expect("parse mini-doc");
        assert_eq!(doc.did.as_str(), "did:plc:hdhoaan3xa3jiuq4fg4mefid");
        assert_eq!(doc.handle.as_str(), "bad-example.com");
        assert_eq!(
            doc.pds.as_ref(),
            "https://porcini.us-east.host.bsky.network"
        );
        assert!(doc.signing_key.as_ref().starts_with('z'));
    }

    #[test]
    fn slingshot_mini_doc_parse_error_status() {
        let buf = Bytes::from_static(
            br#"{
  "error": "RecordNotFound",
  "message": "This record was deleted"
}"#,
        );
        let resp = MiniDocResponse {
            buffer: buf,
            status: StatusCode::BAD_REQUEST,
        };
        match resp.parse() {
            Err(IdentityError::HttpStatus(s)) => assert_eq!(s, StatusCode::BAD_REQUEST),
            other => panic!("unexpected: {:?}", other),
        }
    }
    use crate::client::{HttpClient, XrpcClient};
    use http::Request;
    use jacquard_common::CowStr;

    struct TestXrpc {
        client: reqwest::Client,
    }
    impl TestXrpc {
        fn new() -> Self {
            Self {
                client: reqwest::Client::new(),
            }
        }
    }
    impl HttpClient for TestXrpc {
        type Error = reqwest::Error;
        async fn send_http(
            &self,
            request: Request<Vec<u8>>,
        ) -> Result<http::Response<Vec<u8>>, Self::Error> {
            self.client.send_http(request).await
        }
    }
    impl XrpcClient for TestXrpc {
        fn base_uri(&self) -> CowStr<'_> {
            CowStr::from("https://public.api.bsky.app")
        }
    }
}

/// Resolver specialized for unauthenticated/public flows using reqwest + AuthenticatedClient
pub type PublicResolver = DefaultResolver<AuthenticatedClient<reqwest::Client>>;

impl Default for PublicResolver {
    /// Build a resolver with:
    /// - reqwest HTTP client
    /// - XRPC base https://public.api.bsky.app (unauthenticated)
    /// - default options (DNS enabled if compiled, public fallback for handles enabled)
    ///
    /// Example
    /// ```ignore
    /// use jacquard::identity::resolver::PublicResolver;
    /// let resolver = PublicResolver::default();
    /// ```
    fn default() -> Self {
        let http = reqwest::Client::new();
        let xrpc =
            AuthenticatedClient::new(http.clone(), CowStr::from("https://public.api.bsky.app"));
        let opts = ResolverOptions::default();
        let resolver = DefaultResolver::new(http, xrpc, opts);
        #[cfg(feature = "dns")]
        let resolver = resolver.with_system_dns();
        resolver
    }
}

/// Build a resolver configured to use Slingshot (`https://slingshot.microcosm.blue`) for PLC and
/// mini-doc fallbacks, unauthenticated by default.
pub fn slingshot_resolver_default() -> PublicResolver {
    let http = reqwest::Client::new();
    let xrpc = AuthenticatedClient::new(http.clone(), CowStr::from("https://public.api.bsky.app"));
    let mut opts = ResolverOptions::default();
    opts.plc_source = PlcSource::slingshot_default();
    let resolver = DefaultResolver::new(http, xrpc, opts);
    #[cfg(feature = "dns")]
    let resolver = resolver.with_system_dns();
    resolver
}
