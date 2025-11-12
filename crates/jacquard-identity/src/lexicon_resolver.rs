//! Lexicon schema resolution via DNS and XRPC
//!
//! This module provides traits and implementations for resolving lexicon schemas at runtime:
//! 1. Resolve NSID authority to DID via DNS TXT records (`_lexicon.{reversed-authority}`)
//! 2. Fetch lexicon schema from `com.atproto.lexicon.schema` collection via XRPC

use crate::resolver::{IdentityError, IdentityResolver};

use jacquard_common::{
    IntoStatic, from_json_value, smol_str,
    types::{aturi::AtUri, cid::Cid, did::Did, string::Nsid},
};
use jacquard_lexicon::lexicon::LexiconDoc;
use smol_str::SmolStr;
use url::Url;

/// Resolve lexicon authority (NSID → authoritative DID)
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait LexiconAuthorityResolver {
    /// Resolve an NSID to the authoritative DID via DNS
    ///
    /// Uses DNS TXT records at `_lexicon.{reversed-authority}`, following the
    /// AT Protocol lexicon authority spec. Authority segments are reversed
    /// (e.g., `app.bsky.feed` → query `_lexicon.feed.bsky.app`).
    ///
    /// Note: No hierarchical fallback - per the spec, only exact authority match is checked.
    async fn resolve_lexicon_authority(
        &self,
        nsid: &Nsid,
    ) -> std::result::Result<Did<'static>, LexiconResolutionError>;
}

/// Resolve lexicon schemas (NSID → schema document)
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait LexiconSchemaResolver {
    /// Resolve a complete lexicon schema for an NSID
    async fn resolve_lexicon_schema(
        &self,
        nsid: &Nsid,
    ) -> std::result::Result<ResolvedLexiconSchema<'static>, LexiconResolutionError>;
}

/// A resolved lexicon schema with metadata
#[derive(Debug, Clone)]
pub struct ResolvedLexiconSchema<'s> {
    /// The NSID of the schema
    pub nsid: Nsid<'s>,
    /// DID of the repository this schema was fetched from
    pub repo: Did<'s>,
    /// Content ID of the record (for cache invalidation)
    pub cid: Cid<'s>,
    /// Parsed lexicon document
    pub doc: jacquard_lexicon::lexicon::LexiconDoc<'s>,
}

/// Error type for lexicon resolution operations
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{kind}")]
pub struct LexiconResolutionError {
    #[diagnostic_source]
    kind: LexiconResolutionErrorKind,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl LexiconResolutionError {
    pub fn new(
        kind: LexiconResolutionErrorKind,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self { kind, source }
    }

    pub fn kind(&self) -> &LexiconResolutionErrorKind {
        &self.kind
    }

    pub fn dns_lookup_failed(
        authority: impl Into<SmolStr>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            LexiconResolutionErrorKind::DnsLookupFailed {
                authority: authority.into(),
            },
            Some(Box::new(source)),
        )
    }

    pub fn no_did_found(authority: impl Into<SmolStr>) -> Self {
        Self::new(
            LexiconResolutionErrorKind::NoDIDFound {
                authority: authority.into(),
            },
            None,
        )
    }

    pub fn invalid_did(authority: impl Into<SmolStr>, value: impl Into<SmolStr>) -> Self {
        Self::new(
            LexiconResolutionErrorKind::InvalidDID {
                authority: authority.into(),
                value: value.into(),
            },
            None,
        )
    }

    pub fn dns_not_configured() -> Self {
        Self::new(LexiconResolutionErrorKind::DnsNotConfigured, None)
    }

    pub fn fetch_failed(
        nsid: impl Into<SmolStr>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            LexiconResolutionErrorKind::FetchFailed { nsid: nsid.into() },
            Some(Box::new(source)),
        )
    }

    pub fn parse_failed(
        nsid: impl Into<SmolStr>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            LexiconResolutionErrorKind::ParseFailed { nsid: nsid.into() },
            Some(Box::new(source)),
        )
    }

    pub fn invalid_collection() -> Self {
        Self::new(LexiconResolutionErrorKind::InvalidCollection, None)
    }

    pub fn missing_cid(nsid: impl Into<SmolStr>) -> Self {
        Self::new(
            LexiconResolutionErrorKind::MissingCID { nsid: nsid.into() },
            None,
        )
    }
}

impl From<IdentityError> for LexiconResolutionError {
    fn from(err: IdentityError) -> Self {
        Self::new(LexiconResolutionErrorKind::IdentityResolution(err), None)
    }
}

/// Error categories for lexicon resolution
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum LexiconResolutionErrorKind {
    #[error("DNS lookup failed for authority {authority}")]
    #[diagnostic(code(jacquard::lexicon::dns_lookup_failed))]
    DnsLookupFailed { authority: SmolStr },

    #[error("no DID found in DNS for authority {authority}")]
    #[diagnostic(
        code(jacquard::lexicon::no_did_found),
        help("ensure _lexicon.{{reversed-authority}} TXT record exists with did=...")
    )]
    NoDIDFound { authority: SmolStr },

    #[error("invalid DID in DNS for authority {authority}: {value}")]
    #[diagnostic(code(jacquard::lexicon::invalid_did))]
    InvalidDID { authority: SmolStr, value: SmolStr },

    #[error("DNS not configured (dns feature disabled or WASM target)")]
    #[diagnostic(
        code(jacquard::lexicon::dns_not_configured),
        help("enable the 'dns' feature or use a non-WASM target")
    )]
    DnsNotConfigured,

    #[error("failed to fetch lexicon record for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::fetch_failed))]
    FetchFailed { nsid: SmolStr },

    #[error("failed to parse lexicon schema for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::parse_failed))]
    ParseFailed { nsid: SmolStr },

    #[error("invalid collection NSID")]
    #[diagnostic(code(jacquard::lexicon::invalid_collection))]
    InvalidCollection,

    #[error("record missing CID for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::missing_cid))]
    MissingCID { nsid: SmolStr },

    #[error(transparent)]
    #[diagnostic(code(jacquard::lexicon::identity_resolution_failed))]
    IdentityResolution(#[from] crate::resolver::IdentityError),
}

// Implementation on JacquardResolver
impl crate::JacquardResolver {
    /// Resolve lexicon authority via DNS
    ///
    /// Queries `_lexicon.{reversed-authority}` for a TXT record containing `did=...`
    #[cfg(all(feature = "dns", not(target_family = "wasm")))]
    async fn resolve_lexicon_authority_dns(
        &self,
        nsid: &Nsid<'_>,
    ) -> std::result::Result<Did<'static>, LexiconResolutionError> {
        let Some(dns) = &self.dns else {
            return Err(LexiconResolutionError::dns_not_configured());
        };

        // Extract and reverse authority segments
        let authority = nsid.domain_authority();
        let reversed_authority = authority.split('.').rev().collect::<Vec<_>>().join(".");
        let fqdn = format!("_lexicon.{}.", reversed_authority);

        #[cfg(feature = "tracing")]
        tracing::debug!("resolving lexicon authority via DNS: {}", fqdn);

        let response = dns
            .txt_lookup(fqdn)
            .await
            .map_err(|e| LexiconResolutionError::dns_lookup_failed(authority, e))?;

        // Parse TXT records looking for "did=..."
        for txt in response.iter() {
            for data in txt.txt_data().iter() {
                let text = std::str::from_utf8(data).unwrap_or("");
                if let Some(did_str) = text.strip_prefix("did=") {
                    use jacquard_common::IntoStatic;

                    return Did::new_owned(did_str)
                        .map(|d| d.into_static())
                        .map_err(|_| LexiconResolutionError::invalid_did(authority, did_str));
                }
            }
        }

        Err(LexiconResolutionError::no_did_found(authority))
    }
}

#[allow(unused)]
use jacquard_api::com_atproto::lexicon::resolve_lexicon::{ResolveLexicon, ResolveLexiconOutput};

impl crate::JacquardResolver {
    #[allow(dead_code)]
    async fn resolve_lexicon_xrpc(
        &self,
        nsid: &Nsid<'_>,
    ) -> std::result::Result<ResolvedLexiconSchema<'static>, LexiconResolutionError> {
        if let Ok(mut url) = Url::parse("https://public.api.bsky.app") {
            url.set_path("/xrpc/com.atproto.lexicon.resolveLexicon");
            if let Ok(qs) =
                serde_html_form::to_string(&ResolveLexicon::new().nsid(nsid.clone()).build())
            {
                url.set_query(Some(&qs));
            } else {
                return Err(LexiconResolutionError::invalid_collection());
            }
            if let Ok((buf, status)) = self.get_json_bytes(url).await {
                if status.is_success() {
                    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&buf) {
                        if let Some(obj) = val.as_object() {
                            if let Some(schema) = obj.get("schema") {
                                if let Ok(schema) = from_json_value::<LexiconDoc>(schema.clone()) {
                                    let uri =
                                        obj.get("uri").expect("uri should be present").to_string();
                                    let cid =
                                        obj.get("cid").expect("cid should be present").to_string();
                                    let uri = AtUri::new_owned(uri).map_err(|e| {
                                        LexiconResolutionError::parse_failed("uri", e)
                                    })?;
                                    let cid = Cid::str(&cid).into_static();
                                    let repo = Did::raw(uri.authority().as_str()).into_static();
                                    return Ok(ResolvedLexiconSchema {
                                        repo,
                                        cid,
                                        nsid: nsid.clone().into_static(),
                                        doc: schema.into_static(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(LexiconResolutionError::invalid_collection())
    }
}

#[cfg(all(feature = "dns", not(target_family = "wasm")))]
impl LexiconAuthorityResolver for crate::JacquardResolver {
    async fn resolve_lexicon_authority(
        &self,
        nsid: &Nsid<'_>,
    ) -> std::result::Result<Did<'static>, LexiconResolutionError> {
        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            let authority = jacquard_common::smol_str::SmolStr::from(nsid.domain_authority());
            if let Some(did) = crate::cache_impl::get(&caches.authority_to_did, &authority) {
                return Ok(did);
            }
        }

        // Resolve via DNS
        let result = self.resolve_lexicon_authority_dns(nsid).await;

        // Cache on success, invalidate on error
        match &result {
            Ok(did) =>
            {
                #[cfg(feature = "cache")]
                if let Some(caches) = &self.caches {
                    let authority =
                        jacquard_common::smol_str::SmolStr::from(nsid.domain_authority());
                    crate::cache_impl::insert(&caches.authority_to_did, authority, did.clone());
                }
            }
            Err(_) => {
                #[cfg(feature = "cache")]
                self.invalidate_authority_chain(nsid.domain_authority())
                    .await;
            }
        }

        result
    }
}

#[cfg(not(all(feature = "dns", not(target_family = "wasm"))))]
impl LexiconAuthorityResolver for crate::JacquardResolver {
    async fn resolve_lexicon_authority(
        &self,
        _nsid: &Nsid<'_>,
    ) -> std::result::Result<Did<'static>, LexiconResolutionError> {
        Err(LexiconResolutionError::dns_not_configured())
    }
}

impl LexiconSchemaResolver for crate::JacquardResolver {
    async fn resolve_lexicon_schema(
        &self,
        nsid: &Nsid<'_>,
    ) -> std::result::Result<ResolvedLexiconSchema<'static>, LexiconResolutionError> {
        use jacquard_api::com_atproto::repo::get_record::GetRecord;
        use jacquard_common::{IntoStatic, xrpc::XrpcExt};

        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            let key = nsid.clone().into_static();
            if let Some(schema) = crate::cache_impl::get(&caches.nsid_to_schema, &key) {
                return Ok((*schema).clone());
            }
        }

        // Perform resolution
        #[cfg(feature = "dns")]
        let result = async {
            // 1. Resolve authority DID via DNS
            let authority_did = self.resolve_lexicon_authority(nsid).await?;

            #[cfg(feature = "tracing")]
            tracing::debug!(
                "resolved lexicon authority {} -> {}",
                nsid.domain_authority(),
                authority_did
            );

            // 2. Resolve DID document to get PDS endpoint
            let did_doc_resp = self.resolve_did_doc(&authority_did).await?;
            let did_doc = did_doc_resp.parse()?;
            let pds = did_doc
                .pds_endpoint()
                .ok_or_else(|| IdentityError::missing_pds_endpoint())?;

            #[cfg(feature = "tracing")]
            tracing::debug!("fetching lexicon {} from PDS {}", nsid, pds);

            // 3. Fetch lexicon record via XRPC getRecord
            let collection = Nsid::new("com.atproto.lexicon.schema")
                .map_err(|_| LexiconResolutionError::invalid_collection())?;

            let request = GetRecord::new()
                .repo(authority_did.clone())
                .collection(collection.into_static())
                .rkey(nsid.clone())
                .build();

            let response = self
                .xrpc(pds)
                .send(&request)
                .await
                .map_err(|e| LexiconResolutionError::fetch_failed(nsid.as_str(), e))?;

            let output = response
                .into_output()
                .map_err(|e| LexiconResolutionError::fetch_failed(nsid.as_str(), e))?;

            // 4. Parse lexicon document from value
            let json_str = serde_json::to_string(&output.value)
                .map_err(|e| LexiconResolutionError::parse_failed(nsid.as_str(), e))?;

            let doc: jacquard_lexicon::lexicon::LexiconDoc = serde_json::from_str(&json_str)
                .map_err(|e| LexiconResolutionError::parse_failed(nsid.as_str(), e))?;

            #[cfg(feature = "tracing")]
            tracing::debug!("successfully parsed lexicon schema {}", nsid);

            let cid = output
                .cid
                .ok_or_else(|| LexiconResolutionError::missing_cid(nsid.as_str()))?
                .into_static();

            Ok(ResolvedLexiconSchema {
                nsid: nsid.clone().into_static(),
                repo: authority_did.into_static(),
                cid,
                doc: doc.into_static(),
            })
        }
        .await;

        #[cfg(not(feature = "dns"))]
        let result = self.resolve_lexicon_xrpc(nsid).await;

        // Handle result
        match result {
            Ok(schema) => {
                // Cache successful resolution
                #[cfg(feature = "cache")]
                if let Some(caches) = &self.caches {
                    crate::cache_impl::insert(
                        &caches.nsid_to_schema,
                        nsid.clone().into_static(),
                        std::sync::Arc::new(schema.clone()),
                    );
                }
                Ok(schema)
            }
            Err(e) => {
                // Invalidate on error
                #[cfg(feature = "cache")]
                self.invalidate_lexicon_chain(nsid).await;
                Err(e)
            }
        }
    }
}
