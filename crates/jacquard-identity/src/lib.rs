//! Identity resolution utilities: DID and handle resolution, DID document fetch,
//! and helpers for PDS endpoint discovery. See `identity::resolver` for details.
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

// use crate::CowStr; // not currently needed directly here
pub mod resolver;

use crate::resolver::{
    DidDocResponse, DidStep, HandleStep, IdentityError, IdentityResolver, MiniDoc, PlcSource,
    ResolverOptions,
};
use bytes::Bytes;
use jacquard_api::com_atproto::identity::resolve_did;
use jacquard_api::com_atproto::identity::resolve_handle::ResolveHandle;
use jacquard_common::error::TransportError;
use jacquard_common::http_client::HttpClient;
use jacquard_common::types::did::Did;
use jacquard_common::types::did_doc::DidDocument;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::xrpc::XrpcExt;
use jacquard_common::{IntoStatic, types::string::Handle};
use percent_encoding::percent_decode_str;
use reqwest::StatusCode;
use url::{ParseError, Url};

#[cfg(feature = "dns")]
use hickory_resolver::{TokioAsyncResolver, config::ResolverConfig};

/// Default resolver implementation with configurable fallback order.
pub struct JacquardResolver {
    http: reqwest::Client,
    opts: ResolverOptions,
    #[cfg(feature = "dns")]
    dns: Option<TokioAsyncResolver>,
}

impl JacquardResolver {
    /// Create a new instance of the default resolver with all options (except DNS) up front
    pub fn new(http: reqwest::Client, opts: ResolverOptions) -> Self {
        Self {
            http,
            opts,
            #[cfg(feature = "dns")]
            dns: None,
        }
    }

    #[cfg(feature = "dns")]
    /// Create a new instance of the default resolver with all options, plus default DNS, up front
    pub fn new_dns(http: reqwest::Client, opts: ResolverOptions) -> Self {
        Self {
            http,
            opts,
            dns: Some(TokioAsyncResolver::tokio(
                ResolverConfig::default(),
                Default::default(),
            )),
        }
    }

    #[cfg(feature = "dns")]
    /// Add default DNS resolution to the resolver
    pub fn with_system_dns(mut self) -> Self {
        self.dns = Some(TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            Default::default(),
        ));
        self
    }

    /// Set PLC source (PLC directory or Slingshot)
    pub fn with_plc_source(mut self, source: PlcSource) -> Self {
        self.opts.plc_source = source;
        self
    }

    /// Enable/disable public unauthenticated fallback for resolveHandle
    pub fn with_public_fallback_for_handle(mut self, enable: bool) -> Self {
        self.opts.public_fallback_for_handle = enable;
        self
    }

    /// Enable/disable doc id validation
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
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(TransportError::from)?;
        let status = resp.status();
        let buf = resp.bytes().await.map_err(TransportError::from)?;
        Ok((buf, status))
    }

    async fn get_text(&self, url: Url) -> Result<String, IdentityError> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(TransportError::from)?;
        if resp.status() == StatusCode::OK {
            Ok(resp.text().await.map_err(TransportError::from)?)
        } else {
            Err(IdentityError::Http(
                resp.error_for_status().unwrap_err().into(),
            ))
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

impl JacquardResolver {
    /// Resolve handle to DID via a PDS XRPC call (stateless, unauth by default)
    pub async fn resolve_handle_via_pds(
        &self,
        handle: &Handle<'_>,
    ) -> Result<Did<'static>, IdentityError> {
        let pds = match &self.opts.pds_fallback {
            Some(u) => u.clone(),
            None => return Err(IdentityError::InvalidWellKnown),
        };
        let req = ResolveHandle::new()
            .handle(handle.clone().into_static())
            .build();
        let resp = self
            .http
            .xrpc(pds)
            .send(&req)
            .await
            .map_err(|e| IdentityError::Xrpc(e.to_string()))?;
        let out = resp
            .parse()
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
        let pds = match &self.opts.pds_fallback {
            Some(u) => u.clone(),
            None => return Err(IdentityError::InvalidWellKnown),
        };
        let req = resolve_did::ResolveDid::new().did(did.clone()).build();
        let resp = self
            .http
            .xrpc(pds)
            .send(&req)
            .await
            .map_err(|e| IdentityError::Xrpc(e.to_string()))?;
        let out = resp
            .parse()
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
        if let Ok(qs) = serde_html_form::to_string(
            &resolve_did::ResolveDid::new()
                .did(did.clone().into_static())
                .build(),
        ) {
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

impl IdentityResolver for JacquardResolver {
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
                    // Prefer PDS XRPC via stateless client
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
                        PlcSource::PlcDirectory { base } => {
                            // this is odd, the join screws up with the plc directory but NOT slingshot
                            Url::parse(&format!("{}{}", base, did.as_str())).expect("Invalid URL")
                        }
                        PlcSource::Slingshot { base } => base.join(did.as_str())?,
                    };
                    println!("Fetching DID document from {}", url);
                    if let Ok((buf, status)) = self.get_json_bytes(url).await {
                        return Ok(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                    }
                }
                DidStep::PdsResolveDid => {
                    // Try PDS XRPC for full DID doc
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

impl HttpClient for JacquardResolver {
    async fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> core::result::Result<http::Response<Vec<u8>>, Self::Error> {
        self.http.send_http(request).await
    }

    type Error = reqwest::Error;
}

/// Warnings produced during identity checks that are not fatal
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityWarning {
    /// The DID doc did not contain the expected handle alias under alsoKnownAs
    HandleAliasMismatch {
        #[allow(missing_docs)]
        expected: Handle<'static>,
    },
}

impl JacquardResolver {
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

/// Resolver specialized for unauthenticated/public flows using reqwest and stateless XRPC
pub type PublicResolver = JacquardResolver;

impl Default for PublicResolver {
    /// Build a resolver with:
    /// - reqwest HTTP client
    /// - Public fallbacks enabled for handle resolution
    /// - default options (DNS enabled if compiled, public fallback for handles enabled)
    ///
    /// Example
    /// ```ignore
    /// use jacquard::identity::resolver::PublicResolver;
    /// let resolver = PublicResolver::default();
    /// ```
    fn default() -> Self {
        let http = reqwest::Client::new();
        let opts = ResolverOptions::default();
        let resolver = JacquardResolver::new(http, opts);
        #[cfg(feature = "dns")]
        let resolver = resolver.with_system_dns();
        resolver
    }
}

/// Build a resolver configured to use Slingshot (`https://slingshot.microcosm.blue`) for PLC and
/// mini-doc fallbacks, unauthenticated by default.
pub fn slingshot_resolver_default() -> PublicResolver {
    let http = reqwest::Client::new();
    let mut opts = ResolverOptions::default();
    opts.plc_source = PlcSource::slingshot_default();
    let resolver = JacquardResolver::new(http, opts);
    #[cfg(feature = "dns")]
    let resolver = resolver.with_system_dns();
    resolver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_web_urls() {
        let r = JacquardResolver::new(reqwest::Client::new(), ResolverOptions::default());
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
    fn slingshot_mini_doc_url_build() {
        let r = JacquardResolver::new(reqwest::Client::new(), ResolverOptions::default());
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
}
