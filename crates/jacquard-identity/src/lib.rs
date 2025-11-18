//! Identity resolution for the AT Protocol
//!
//! Jacquard's handle-to-DID and DID-to-document resolution with configurable
//! fallback chains.
//!
//! ## Quick start
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use jacquard_identity::{PublicResolver, resolver::IdentityResolver};
//! use jacquard_common::types::string::Handle;
//!
//! let resolver = PublicResolver::default();
//!
//! // Resolve handle to DID
//! let did = resolver.resolve_handle(&Handle::new("alice.bsky.social")?).await?;
//!
//! // Fetch DID document
//! let doc_response = resolver.resolve_did_doc(&did).await?;
//! let doc = doc_response.parse()?;  // Borrow from response buffer
//! # Ok(())
//! # }
//! ```
//!
//! ## Resolution fallback order
//!
//! **Handle → DID** (configurable via [`resolver::HandleStep`]):
//! 1. DNS TXT record at `_atproto.{handle}` (if `dns` feature enabled)
//! 2. HTTPS well-known at `https://{handle}/.well-known/atproto-did`
//! 3. PDS XRPC `com.atproto.identity.resolveHandle` (if PDS configured)
//! 4. Public API fallback (`https://public.api.bsky.app`)
//! 5. Slingshot `resolveHandle` (if configured)
//!
//! **DID → Document** (configurable via [`resolver::DidStep`]):
//! 1. `did:web` HTTPS well-known
//! 2. PLC directory HTTP (for `did:plc`)
//! 3. PDS XRPC `com.atproto.identity.resolveDid` (if PDS configured)
//! 4. Slingshot mini-doc (partial document)
//!
//! ## Customization
//!
//! ```
//! use jacquard_identity::JacquardResolver;
//! use jacquard_identity::resolver::{ResolverOptions, PlcSource};
//!
//! let opts = ResolverOptions {
//!     plc_source: PlcSource::slingshot_default(),
//!     public_fallback_for_handle: true,
//!     validate_doc_id: true,
//!     ..Default::default()
//! };
//!
//! let resolver = JacquardResolver::new(reqwest::Client::new(), opts);
//! #[cfg(feature = "dns")]
//! let resolver = resolver.with_system_dns();  // Enable DNS TXT resolution
//! ```
//!
//! ## Response types
//!
//! Resolution methods return wrapper types that own the response buffer, allowing
//! zero-copy parsing:
//!
//! - [`resolver::DidDocResponse`] - Full DID document response
//! - [`MiniDocResponse`] - Slingshot mini-doc response (partial)
//!
//! Both support `.parse()` for borrowing and validation.

// use crate::CowStr; // not currently needed directly here

#![cfg_attr(target_arch = "wasm32", allow(unused))]
pub mod lexicon_resolver;
pub mod resolver;

use crate::resolver::{
    DidDocResponse, DidStep, HandleStep, IdentityError, IdentityResolver, MiniDoc, PlcSource,
    ResolverOptions,
};
use bytes::Bytes;
use jacquard_api::com_atproto::identity::resolve_did;
use jacquard_api::com_atproto::identity::resolve_handle::ResolveHandle;
#[cfg(feature = "streaming")]
use jacquard_common::ByteStream;
use jacquard_common::http_client::HttpClient;
use jacquard_common::types::did::Did;
use jacquard_common::types::did_doc::DidDocument;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::xrpc::XrpcExt;
use jacquard_common::{IntoStatic, types::string::Handle};
use percent_encoding::percent_decode_str;
use reqwest::StatusCode;
use url::{ParseError, Url};

#[cfg(all(feature = "dns", not(target_family = "wasm")))]
use {
    hickory_resolver::{TokioAsyncResolver, config::ResolverConfig},
    std::sync::Arc,
};

#[cfg(feature = "cache")]
use {
    crate::lexicon_resolver::ResolvedLexiconSchema,
    jacquard_common::{smol_str::SmolStr, types::string::Nsid},
    mini_moka::time::Duration,
};

#[cfg(all(
    feature = "cache",
    not(all(feature = "dns", not(target_family = "wasm")))
))]
use std::sync::Arc;

// Platform-specific cache implementations
//#[cfg(all(feature = "cache", not(target_arch = "wasm32")))]
#[cfg(feature = "cache")]
mod cache_impl {
    /// Native: Use sync cache (thread-safe, no mutex needed)
    pub type Cache<K, V> = mini_moka::sync::Cache<K, V>;

    pub fn new_cache<K, V>(max_capacity: u64, ttl: std::time::Duration) -> Cache<K, V>
    where
        K: std::hash::Hash + Eq + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        mini_moka::sync::Cache::builder()
            .max_capacity(max_capacity)
            .time_to_idle(ttl)
            .build()
    }

    pub fn get<K, V>(cache: &Cache<K, V>, key: &K) -> Option<V>
    where
        K: std::hash::Hash + Eq + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        cache.get(key)
    }

    pub fn insert<K, V>(cache: &Cache<K, V>, key: K, value: V)
    where
        K: std::hash::Hash + Eq + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        cache.insert(key, value);
    }

    pub fn invalidate<K, V>(cache: &Cache<K, V>, key: &K)
    where
        K: std::hash::Hash + Eq + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        cache.invalidate(key);
    }
}

// #[cfg(all(feature = "cache", target_arch = "wasm32"))]
// mod cache_impl {
//     use std::sync::{Arc, Mutex};

//     /// WASM: Use unsync cache in Arc<Mutex<_>> (no threads, but need interior mutability)
//     pub type Cache<K, V> = Arc<Mutex<mini_moka::unsync::Cache<K, V>>>;

//     pub fn new_cache<K, V>(max_capacity: u64, ttl: std::time::Duration) -> Cache<K, V>
//     where
//         K: std::hash::Hash + Eq + 'static,
//         V: Clone + 'static,
//     {
//         Arc::new(Mutex::new(
//             mini_moka::unsync::Cache::builder()
//                 .max_capacity(max_capacity)
//                 .time_to_idle(ttl)
//                 .build(),
//         ))
//     }

//     pub fn get<K, V>(cache: &Cache<K, V>, key: &K) -> Option<V>
//     where
//         K: std::hash::Hash + Eq + 'static,
//         V: Clone + 'static,
//     {
//         cache.lock().unwrap().get(key).cloned()
//     }

//     pub fn insert<K, V>(cache: &Cache<K, V>, key: K, value: V)
//     where
//         K: std::hash::Hash + Eq + 'static,
//         V: Clone + 'static,
//     {
//         cache.lock().unwrap().insert(key, value);
//     }

//     pub fn invalidate<K, V>(cache: &Cache<K, V>, key: &K)
//     where
//         K: std::hash::Hash + Eq + 'static,
//         V: Clone + 'static,
//     {
//         cache.lock().unwrap().invalidate(key);
//     }
// }

/// Configuration for resolver caching
#[cfg(feature = "cache")]
#[derive(Clone, Debug)]
pub struct CacheConfig {
    /// Maximum capacity for handle→DID cache
    pub handle_to_did_capacity: u64,
    /// TTL for handle→DID cache
    pub handle_to_did_ttl: Duration,
    /// Maximum capacity for DID→document cache
    pub did_to_doc_capacity: u64,
    /// TTL for DID→document cache
    pub did_to_doc_ttl: Duration,
    /// Maximum capacity for authority→DID cache
    pub authority_to_did_capacity: u64,
    /// TTL for authority→DID cache
    pub authority_to_did_ttl: Duration,
    /// Maximum capacity for NSID→schema cache
    pub nsid_to_schema_capacity: u64,
    /// TTL for NSID→schema cache
    pub nsid_to_schema_ttl: Duration,
}

#[cfg(feature = "cache")]
impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            handle_to_did_capacity: 2000,
            handle_to_did_ttl: Duration::from_secs(24 * 3600),
            did_to_doc_capacity: 1000,
            did_to_doc_ttl: Duration::from_secs(72 * 3600),
            authority_to_did_capacity: 1000,
            authority_to_did_ttl: Duration::from_secs(168 * 3600),
            nsid_to_schema_capacity: 1000,
            nsid_to_schema_ttl: Duration::from_secs(168 * 3600),
        }
    }
}

#[cfg(feature = "cache")]
impl CacheConfig {
    /// Set handle→DID cache parameters
    pub fn with_handle_cache(mut self, capacity: u64, ttl: Duration) -> Self {
        self.handle_to_did_capacity = capacity;
        self.handle_to_did_ttl = ttl;
        self
    }

    /// Set DID→document cache parameters
    pub fn with_did_doc_cache(mut self, capacity: u64, ttl: Duration) -> Self {
        self.did_to_doc_capacity = capacity;
        self.did_to_doc_ttl = ttl;
        self
    }

    /// Set authority→DID cache parameters
    pub fn with_authority_cache(mut self, capacity: u64, ttl: Duration) -> Self {
        self.authority_to_did_capacity = capacity;
        self.authority_to_did_ttl = ttl;
        self
    }

    /// Set NSID→schema cache parameters
    pub fn with_schema_cache(mut self, capacity: u64, ttl: Duration) -> Self {
        self.nsid_to_schema_capacity = capacity;
        self.nsid_to_schema_ttl = ttl;
        self
    }
}

/// Cache layer for resolver operations
///
/// Fairly simple, in-memory only. If you want something more complex with persistence,
/// implemement the appropriate resolver traits on your own struct, or wrap
/// JacquardResolver in a custom cache layer. The intent here is to allow your
/// backend service to not hammer people's DNS or PDS/entryway if you make requests
/// that need to do resolution first (e.g. the get_record helper functions), not
/// to provide a complete caching solution for all use cases of the resolver.
///
/// **Note from the author:** If there is desire or need, I can break out cache operation
/// functions into a trait to make this more pluggable, but this solves the typical
/// use case.
#[cfg(feature = "cache")]
#[derive(Clone)]
pub struct ResolverCaches {
    pub handle_to_did: cache_impl::Cache<Handle<'static>, Did<'static>>,
    pub did_to_doc: cache_impl::Cache<Did<'static>, Arc<DidDocResponse>>,
    pub authority_to_did: cache_impl::Cache<SmolStr, Did<'static>>,
    pub nsid_to_schema: cache_impl::Cache<Nsid<'static>, Arc<ResolvedLexiconSchema<'static>>>,
}

#[cfg(feature = "cache")]
impl ResolverCaches {
    pub fn new(config: &CacheConfig) -> Self {
        Self {
            handle_to_did: cache_impl::new_cache(
                config.handle_to_did_capacity,
                config.handle_to_did_ttl,
            ),
            did_to_doc: cache_impl::new_cache(config.did_to_doc_capacity, config.did_to_doc_ttl),
            authority_to_did: cache_impl::new_cache(
                config.authority_to_did_capacity,
                config.authority_to_did_ttl,
            ),
            nsid_to_schema: cache_impl::new_cache(
                config.nsid_to_schema_capacity,
                config.nsid_to_schema_ttl,
            ),
        }
    }
}

#[cfg(feature = "cache")]
impl Default for ResolverCaches {
    fn default() -> Self {
        Self::new(&CacheConfig::default())
    }
}

/// Default resolver implementation with configurable fallback order.
#[derive(Clone)]
pub struct JacquardResolver {
    http: reqwest::Client,
    opts: ResolverOptions,
    #[cfg(feature = "dns")]
    dns: Option<Arc<TokioAsyncResolver>>,
    #[cfg(feature = "cache")]
    caches: Option<ResolverCaches>,
}

impl JacquardResolver {
    /// Create a new instance of the default resolver with all options (except DNS) up front
    pub fn new(http: reqwest::Client, opts: ResolverOptions) -> Self {
        // #[cfg(feature = "tracing")]
        // tracing::info!(
        //     public_fallback = opts.public_fallback_for_handle,
        //     validate_doc_id = opts.validate_doc_id,
        //     plc_source = ?opts.plc_source,
        //     "jacquard resolver created"
        // );

        Self {
            http,
            opts,
            #[cfg(feature = "dns")]
            dns: None,
            #[cfg(feature = "cache")]
            caches: None,
        }
    }

    #[cfg(feature = "dns")]
    /// Create a new instance of the default resolver with all options, plus default DNS, up front
    pub fn new_dns(http: reqwest::Client, opts: ResolverOptions) -> Self {
        Self {
            http,
            opts,
            dns: Some(Arc::new(TokioAsyncResolver::tokio(
                ResolverConfig::default(),
                Default::default(),
            ))),
            #[cfg(feature = "cache")]
            caches: None,
        }
    }

    #[cfg(feature = "dns")]
    /// Add default DNS resolution to the resolver
    pub fn with_system_dns(mut self) -> Self {
        self.dns = Some(Arc::new(TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            Default::default(),
        )));
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

    #[cfg(feature = "cache")]
    /// Enable caching with default configuration
    pub fn with_cache(mut self) -> Self {
        self.caches = Some(ResolverCaches::default());
        self
    }

    #[cfg(feature = "cache")]
    /// Enable caching with custom configuration
    pub fn with_cache_config(mut self, config: CacheConfig) -> Self {
        self.caches = Some(ResolverCaches::new(&config));
        self
    }

    /// Construct the well-known HTTPS URL for a `did:web` DID.
    ///
    /// - `did:web:example.com` → `https://example.com/.well-known/did.json`
    /// - `did:web:example.com:user:alice` → `https://example.com/user/alice/did.json`
    fn did_web_url(&self, did: &Did<'_>) -> resolver::Result<Url> {
        // did:web:example.com[:path:segments]
        let s = did.as_str();
        let rest = s
            .strip_prefix("did:web:")
            .ok_or_else(|| IdentityError::unsupported_did_method(s))?;
        let mut parts = rest.split(':');
        let host = parts
            .next()
            .ok_or_else(|| IdentityError::unsupported_did_method(s))?;
        let mut url = Url::parse(&format!("https://{host}/"))?;
        let path: Vec<&str> = parts.collect();
        if path.is_empty() {
            url.set_path(".well-known/did.json");
        } else {
            // Append path segments and did.json
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| IdentityError::url(ParseError::SetHostOnCannotBeABaseUrl))?;
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

    async fn get_json_bytes(&self, url: Url) -> resolver::Result<(Bytes, StatusCode)> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let buf = resp.bytes().await?;
        Ok((buf, status))
    }

    async fn get_text(&self, url: Url) -> resolver::Result<String> {
        let resp = self.http.get(url).send().await?;
        if resp.status() == StatusCode::OK {
            Ok(resp.text().await?)
        } else {
            Err(IdentityError::transport(
                resp.error_for_status().unwrap_err(),
            ))
        }
    }

    #[cfg(feature = "dns")]
    async fn dns_txt(&self, name: &str) -> resolver::Result<Vec<String>> {
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

    /// Query DNS via DNS-over-HTTPS using Cloudflare
    pub async fn query_dns_doh(
        &self,
        name: &str,
        record_type: &str,
    ) -> resolver::Result<serde_json::Value> {
        #[cfg(feature = "tracing")]
        tracing::trace!("querying DNS via DoH: {} ({})", name, record_type);

        let mut url = Url::parse("https://cloudflare-dns.com/dns-query")
            .expect("hardcoded URL should be valid");

        url.query_pairs_mut()
            .append_pair("name", name)
            .append_pair("type", record_type);

        let response = self
            .http
            .get(url)
            .header("Accept", "application/dns-json")
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(IdentityError::http_status(status));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json)
    }

    #[cfg(not(feature = "dns"))]
    async fn dns_txt(&self, name: &str) -> resolver::Result<Vec<String>> {
        let fqdn = format!("_atproto.{name}.");
        let response = self
            .query_dns_doh(&fqdn, "TXT")
            .await
            .map_err(|e| IdentityError::dns(e))?;

        // Parse DoH JSON response
        let answers = response
            .get("Answer")
            .and_then(|a| a.as_array())
            .ok_or_else(|| {
                IdentityError::invalid_well_known().with_context(format!(
                    "couldn't parse cloudflare DoH answers looking for {name}"
                ))
            })?;

        let mut results: Vec<String> = Vec::new();
        for answer in answers {
            if let Some(data) = answer.get("data").and_then(|d| d.as_str()) {
                // TXT records are quoted in DNS responses, strip quotes
                results.push(data.trim_matches('"').to_string())
            }
        }
        Ok(results)
    }

    fn parse_atproto_did_body(body: &str) -> resolver::Result<Did<'static>> {
        let line = body
            .lines()
            .find(|l| !l.trim().is_empty())
            .ok_or_else(|| IdentityError::invalid_well_known())?;
        let did = Did::new(line.trim()).map_err(|_| IdentityError::invalid_well_known())?;
        Ok(did.into_static())
    }
}

impl JacquardResolver {
    /// Resolve handle to DID via a PDS XRPC call (stateless, unauth by default)
    pub async fn resolve_handle_via_pds(
        &self,
        handle: &Handle<'_>,
    ) -> resolver::Result<Did<'static>> {
        let pds = match &self.opts.pds_fallback {
            Some(u) => u.clone(),
            None => return Err(IdentityError::invalid_well_known()),
        };
        let req = ResolveHandle::new()
            .handle(handle.clone().into_static())
            .build();
        let resp = self
            .http
            .xrpc(pds)
            .send(&req)
            .await
            .map_err(|e| IdentityError::xrpc(e.to_string()))?;
        let out = resp
            .parse()
            .map_err(|e| IdentityError::xrpc(e.to_string()))?;
        Did::new_owned(out.did.as_str())
            .map(|d| d.into_static())
            .map_err(|_| IdentityError::invalid_well_known())
    }

    /// Fetch DID document via PDS resolveDid (returns owned DidDocument)
    pub async fn fetch_did_doc_via_pds_owned(
        &self,
        did: &Did<'_>,
    ) -> resolver::Result<DidDocument<'static>> {
        let pds = match &self.opts.pds_fallback {
            Some(u) => u.clone(),
            None => return Err(IdentityError::invalid_well_known()),
        };
        let req = resolve_did::ResolveDid::new().did(did.clone()).build();
        let resp = self
            .http
            .xrpc(pds)
            .send(&req)
            .await
            .map_err(|e| IdentityError::xrpc(e.to_string()))?;
        let out = resp
            .parse()
            .map_err(|e| IdentityError::xrpc(e.to_string()))?;
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
    ) -> resolver::Result<DidDocResponse> {
        let base = match &self.opts.plc_source {
            PlcSource::Slingshot { base } => base.clone(),
            _ => {
                return Err(IdentityError::unsupported_did_method(
                    "mini-doc requires Slingshot source",
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
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug", skip(self), fields(handle = %handle)))]
    async fn resolve_handle(&self, handle: &Handle<'_>) -> resolver::Result<Did<'static>> {
        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            let key = handle.clone().into_static();
            if let Some(did) = cache_impl::get(&caches.handle_to_did, &key) {
                return Ok(did);
            }
        }

        let host = handle.as_str();
        let mut resolved_did: Option<Did<'static>> = None;

        'outer: for step in &self.opts.handle_order {
            match step {
                HandleStep::DnsTxt => {
                    if let Ok(txts) = self.dns_txt(host).await {
                        for txt in txts {
                            if let Some(did_str) = txt.strip_prefix("did=") {
                                if let Ok(did) = Did::new(did_str) {
                                    resolved_did = Some(did.into_static());
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
                HandleStep::HttpsWellKnown => {
                    let url = Url::parse(&format!("https://{host}/.well-known/atproto-did"))?;
                    if let Ok(text) = self.get_text(url).await {
                        if let Ok(did) = Self::parse_atproto_did_body(&text) {
                            resolved_did = Some(did);
                            break 'outer;
                        }
                    }
                }
                HandleStep::PdsResolveHandle => {
                    // Prefer PDS XRPC via stateless client
                    if let Ok(did) = self.resolve_handle_via_pds(handle).await {
                        resolved_did = Some(did);
                        break 'outer;
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
                                                resolved_did = Some(did.into_static());
                                                break 'outer;
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
                                            resolved_did = Some(did.into_static());
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Handle result
        if let Some(did) = resolved_did {
            // Cache successful resolution
            #[cfg(feature = "cache")]
            if let Some(caches) = &self.caches {
                cache_impl::insert(
                    &caches.handle_to_did,
                    handle.clone().into_static(),
                    did.clone(),
                );
            }
            Ok(did)
        } else {
            // Invalidate on error
            #[cfg(feature = "cache")]
            self.invalidate_handle_chain(handle).await;
            Err(IdentityError::invalid_well_known())
        }
    }

    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug", skip(self), fields(did = %did)))]
    async fn resolve_did_doc(&self, did: &Did<'_>) -> resolver::Result<DidDocResponse> {
        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            let key = did.clone().into_static();
            if let Some(doc_resp) = cache_impl::get(&caches.did_to_doc, &key) {
                return Ok((*doc_resp).clone());
            }
        }

        let s = did.as_str();
        let mut resolved_doc: Option<DidDocResponse> = None;

        'outer: for step in &self.opts.did_order {
            match step {
                DidStep::DidWebHttps if s.starts_with("did:web:") => {
                    let url = self.did_web_url(did)?;
                    if let Ok((buf, status)) = self.get_json_bytes(url).await {
                        resolved_doc = Some(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                        break 'outer;
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
                    if let Ok((buf, status)) = self.get_json_bytes(url).await {
                        resolved_doc = Some(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                        break 'outer;
                    }
                }
                DidStep::PdsResolveDid => {
                    // Try PDS XRPC for full DID doc
                    if let Ok(doc) = self.fetch_did_doc_via_pds_owned(did).await {
                        let buf = serde_json::to_vec(&doc).unwrap_or_default();
                        resolved_doc = Some(DidDocResponse {
                            buffer: Bytes::from(buf),
                            status: StatusCode::OK,
                            requested: Some(did.clone().into_static()),
                        });
                        break 'outer;
                    }
                    // Fallback: if Slingshot configured, return mini-doc response (partial doc)
                    if let PlcSource::Slingshot { base } = &self.opts.plc_source {
                        let url = self.slingshot_mini_doc_url(base, did.as_str())?;
                        let (buf, status) = self.get_json_bytes(url).await?;
                        resolved_doc = Some(DidDocResponse {
                            buffer: buf,
                            status,
                            requested: Some(did.clone().into_static()),
                        });
                        break 'outer;
                    }
                }
                _ => {}
            }
        }

        // Handle result
        if let Some(doc_resp) = resolved_doc {
            // Cache successful resolution
            #[cfg(feature = "cache")]
            if let Some(caches) = &self.caches {
                cache_impl::insert(
                    &caches.did_to_doc,
                    did.clone().into_static(),
                    Arc::new(doc_resp.clone()),
                );
            }
            Ok(doc_resp)
        } else {
            // Invalidate on error
            #[cfg(feature = "cache")]
            self.invalidate_did_chain(did).await;
            Err(IdentityError::unsupported_did_method(s))
        }
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

#[cfg(feature = "streaming")]
impl jacquard_common::http_client::HttpClientExt for JacquardResolver {
    /// Send HTTP request and return streaming response
    fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = Result<http::Response<ByteStream>, Self::Error>> {
        self.http.send_http_streaming(request)
    }

    /// Send HTTP request with streaming body and receive streaming response
    #[cfg(not(target_arch = "wasm32"))]
    fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> impl Future<Output = Result<http::Response<ByteStream>, Self::Error>>
    where
        S: n0_future::Stream<Item = Result<bytes::Bytes, jacquard_common::StreamError>>
            + Send
            + 'static,
    {
        self.http.send_http_bidirectional(parts, body)
    }

    /// Send HTTP request with streaming body and receive streaming response (WASM)
    #[cfg(target_arch = "wasm32")]
    fn send_http_bidirectional<S>(
        &self,
        parts: http::request::Parts,
        body: S,
    ) -> impl Future<Output = Result<http::Response<ByteStream>, Self::Error>>
    where
        S: n0_future::Stream<Item = Result<bytes::Bytes, jacquard_common::StreamError>> + 'static,
    {
        self.http.send_http_bidirectional(parts, body)
    }
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
    ) -> resolver::Result<(Did<'static>, DidDocResponse, Vec<IdentityWarning>)> {
        let did = self.resolve_handle(handle).await?;
        let resp = self.resolve_did_doc(&did).await?;
        let resp_for_parse = resp.clone();
        let doc_borrowed = resp_for_parse.parse()?;
        if self.opts.validate_doc_id && doc_borrowed.id.as_str() != did.as_str() {
            return Err(IdentityError::doc_id_mismatch(
                did.clone().into_static(),
                doc_borrowed.clone().into_static(),
            ));
        }
        let mut warnings = Vec::new();
        // Check handle alias presence (soft warning)
        let has_alias = doc_borrowed
            .also_known_as
            .as_ref()
            .map(|v| {
                v.iter().any(|s| {
                    let s = s.strip_prefix("at://").unwrap_or(s);
                    s == handle.as_str()
                })
            })
            .unwrap_or(false);
        if !has_alias {
            warnings.push(IdentityWarning::HandleAliasMismatch {
                expected: handle.clone().into_static(),
            });
        }
        Ok((did, resp, warnings))
    }

    /// Build Slingshot mini-doc URL for an identifier (handle or DID)
    fn slingshot_mini_doc_url(&self, base: &Url, identifier: &str) -> resolver::Result<Url> {
        let mut url = base.clone();
        url.set_path("/xrpc/com.bad-example.identity.resolveMiniDoc");
        url.set_query(Some(&format!(
            "identifier={}",
            urlencoding::Encoded::new(identifier)
        )));
        Ok(url)
    }

    #[cfg(feature = "cache")]
    async fn invalidate_handle_chain(&self, handle: &Handle<'_>) {
        if let Some(caches) = &self.caches {
            let key = handle.clone().into_static();
            cache_impl::invalidate(&caches.handle_to_did, &key);
        }
    }

    #[cfg(feature = "cache")]
    async fn invalidate_did_chain(&self, did: &Did<'_>) {
        if let Some(caches) = &self.caches {
            let did_key = did.clone().into_static();
            // Get doc before evicting to extract handles
            if let Some(doc_resp) = cache_impl::get(&caches.did_to_doc, &did_key) {
                let doc_resp_clone = (*doc_resp).clone();
                if let Ok(doc) = doc_resp_clone.parse() {
                    if let Some(aliases) = &doc.also_known_as {
                        for alias in aliases {
                            if let Some(handle_str) = alias.as_ref().strip_prefix("at://") {
                                if let Ok(handle) = Handle::new(handle_str) {
                                    let handle_key = handle.into_static();
                                    cache_impl::invalidate(&caches.handle_to_did, &handle_key);
                                }
                            }
                        }
                    }
                }
            }
            cache_impl::invalidate(&caches.did_to_doc, &did_key);
        }
    }

    #[cfg(feature = "cache")]
    async fn invalidate_authority_chain(&self, authority: &str) {
        if let Some(caches) = &self.caches {
            let authority = SmolStr::from(authority);
            cache_impl::invalidate(&caches.authority_to_did, &authority);
        }
    }

    #[cfg(feature = "cache")]
    async fn invalidate_lexicon_chain(&self, nsid: &jacquard_common::types::string::Nsid<'_>) {
        if let Some(caches) = &self.caches {
            let nsid_key = nsid.clone().into_static();
            if let Some(schema) = cache_impl::get(&caches.nsid_to_schema, &nsid_key) {
                let authority = SmolStr::from(nsid.domain_authority());
                cache_impl::invalidate(&caches.authority_to_did, &authority);
                self.invalidate_did_chain(&schema.repo).await;
            }
            cache_impl::invalidate(&caches.nsid_to_schema, &nsid_key);
        }
    }

    /// Fetch a minimal DID document via Slingshot's mini-doc endpoint using a generic at-identifier
    pub async fn fetch_mini_doc_via_slingshot_identifier(
        &self,
        identifier: &AtIdentifier<'_>,
    ) -> resolver::Result<MiniDocResponse> {
        let base = match &self.opts.plc_source {
            PlcSource::Slingshot { base } => base.clone(),
            _ => {
                return Err(IdentityError::unsupported_did_method(
                    "mini-doc requires Slingshot source",
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
    pub fn parse<'b>(&'b self) -> resolver::Result<MiniDoc<'b>> {
        if self.status.is_success() {
            serde_json::from_slice::<MiniDoc<'b>>(&self.buffer).map_err(IdentityError::from)
        } else {
            Err(IdentityError::http_status(self.status))
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
        #[cfg(feature = "cache")]
        let resolver = resolver.with_cache();
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
    #[cfg(feature = "cache")]
    let resolver = resolver.with_cache();
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
            Err(e) => match e.kind() {
                resolver::IdentityErrorKind::HttpStatus(s) => {
                    assert_eq!(*s, StatusCode::BAD_REQUEST)
                }
                _ => panic!("unexpected error kind: {:?}", e),
            },
            other => panic!("unexpected: {:?}", other),
        }
    }
}
