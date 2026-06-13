//! Lexicon schema resolution via DNS and XRPC
//!
//! This module provides traits and implementations for resolving lexicon schemas at runtime:
//! 1. Resolve NSID authority to DID via DNS TXT records (`_lexicon.{reversed-authority}`)
//! 2. Fetch lexicon schema from `com.atproto.lexicon.schema` collection via XRPC

use crate::resolver::{IdentityError, IdentityResolver};

use jacquard_common::{
    BosStr,
    deps::smol_str,
    http_client::HttpClient,
    types::{cid::Cid, did::Did, ident::AtIdentifier, string::Nsid, string::RecordKey},
};
use smol_str::SmolStr;

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
    fn resolve_lexicon_authority<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> impl Future<Output = std::result::Result<Did, LexiconResolutionError>>;
}

/// Resolve lexicon schemas (NSID → schema document)
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait LexiconSchemaResolver {
    /// Resolve a complete lexicon schema for an NSID
    fn resolve_lexicon_schema<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> impl Future<Output = std::result::Result<ResolvedLexiconSchema<'static>, LexiconResolutionError>>;
}

/// A resolved lexicon schema with metadata
#[derive(Debug, Clone)]
pub struct ResolvedLexiconSchema<'s> {
    /// The NSID of the schema
    pub nsid: Nsid,
    /// DID of the repository this schema was fetched from
    pub repo: Did,
    /// Content ID of the record (for cache invalidation)
    pub cid: Cid,
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
    context: Option<SmolStr>,
}

impl LexiconResolutionError {
    /// Create a new error with the given kind and optional source.
    pub fn new(
        kind: LexiconResolutionErrorKind,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self {
            kind,
            source,
            context: None,
        }
    }

    /// Return the error kind.
    pub fn kind(&self) -> &LexiconResolutionErrorKind {
        &self.kind
    }

    /// Add context to this error
    pub fn with_context(mut self, context: impl Into<SmolStr>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Get the context if present
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    /// Create an error for a failed DNS TXT lookup while resolving a lexicon authority.
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

    /// Create an error for when DNS records exist but contain no `did=...` entry.
    pub fn no_did_found(authority: impl Into<SmolStr>) -> Self {
        Self::new(
            LexiconResolutionErrorKind::NoDIDFound {
                authority: authority.into(),
            },
            None,
        )
    }

    /// Create an error for a syntactically invalid DID found in DNS for the given authority.
    pub fn invalid_did(authority: impl Into<SmolStr>, value: impl Into<SmolStr>) -> Self {
        Self::new(
            LexiconResolutionErrorKind::InvalidDID {
                authority: authority.into(),
                value: value.into(),
            },
            None,
        )
    }

    /// Create an error for when DNS is not available (feature disabled or WASM target).
    pub fn dns_not_configured() -> Self {
        Self::new(LexiconResolutionErrorKind::DnsNotConfigured, None)
    }

    /// Create an error for a failure to fetch the lexicon record for an NSID.
    pub fn fetch_failed(
        nsid: impl Into<SmolStr>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            LexiconResolutionErrorKind::FetchFailed { nsid: nsid.into() },
            Some(Box::new(source)),
        )
    }

    /// Create an error for a failure to parse a fetched lexicon schema document.
    pub fn parse_failed(
        nsid: impl Into<SmolStr>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::new(
            LexiconResolutionErrorKind::ParseFailed { nsid: nsid.into() },
            Some(Box::new(source)),
        )
    }

    /// Create a generic resolution failure error with a descriptive message.
    pub fn resolution_failed(nsid: impl Into<SmolStr>, message: impl Into<SmolStr>) -> Self {
        Self::new(
            LexiconResolutionErrorKind::ResolutionFailed {
                nsid: nsid.into(),
                message: message.into(),
            },
            None,
        )
    }

    /// Create an error for a non-success HTTP status received while fetching a lexicon.
    pub fn http_error(nsid: impl Into<SmolStr>, status: u16) -> Self {
        Self::new(
            LexiconResolutionErrorKind::HttpError {
                nsid: nsid.into(),
                status,
            },
            None,
        )
    }

    /// Create an error for a required field missing from the XRPC response.
    pub fn missing_response_field(nsid: impl Into<SmolStr>, field: &'static str) -> Self {
        Self::new(
            LexiconResolutionErrorKind::MissingResponseField {
                nsid: nsid.into(),
                field,
            },
            None,
        )
    }

    /// Create an error for an invalid lexicon collection NSID.
    pub fn invalid_collection() -> Self {
        Self::new(LexiconResolutionErrorKind::InvalidCollection, None)
    }

    /// Create an error for a lexicon record response that is missing its CID.
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
#[non_exhaustive]
pub enum LexiconResolutionErrorKind {
    /// DNS TXT lookup for the lexicon authority failed.
    #[error("DNS lookup failed for authority {authority}")]
    #[diagnostic(code(jacquard::lexicon::dns_lookup_failed))]
    DnsLookupFailed {
        /// The NSID authority segment that was being looked up.
        authority: SmolStr,
    },

    /// DNS records were reachable but contained no `did=...` entry.
    #[error("no DID found in DNS for authority {authority}")]
    #[diagnostic(
        code(jacquard::lexicon::no_did_found),
        help("ensure _lexicon.{{reversed-authority}} TXT record exists with did=...")
    )]
    NoDIDFound {
        /// The NSID authority segment that was being looked up.
        authority: SmolStr,
    },

    /// DNS returned a `did=...` entry but its value is not a valid DID.
    #[error("invalid DID in DNS for authority {authority}: {value}")]
    #[diagnostic(code(jacquard::lexicon::invalid_did))]
    InvalidDID {
        /// The NSID authority segment.
        authority: SmolStr,
        /// The raw invalid DID string found in DNS.
        value: SmolStr,
    },

    /// DNS is not available on this build (the `dns` feature is disabled or target is WASM).
    #[error("DNS not configured (dns feature disabled or WASM target)")]
    #[diagnostic(
        code(jacquard::lexicon::dns_not_configured),
        help("enable the 'dns' feature or use a non-WASM target")
    )]
    DnsNotConfigured,

    /// XRPC or HTTP request to fetch the lexicon record failed.
    #[error("failed to fetch lexicon record for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::fetch_failed))]
    FetchFailed {
        /// The NSID of the lexicon that could not be fetched.
        nsid: SmolStr,
    },

    /// The fetched lexicon record could not be deserialized as a `LexiconDoc`.
    #[error("failed to parse lexicon schema for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::parse_failed))]
    ParseFailed {
        /// The NSID of the lexicon that could not be parsed.
        nsid: SmolStr,
    },

    /// Generic resolution failure with a descriptive message.
    #[error("failed to parse lexicon schema for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::resolution_failed))]
    ResolutionFailed {
        /// The NSID of the lexicon being resolved.
        nsid: SmolStr,
        /// Human-readable description of what went wrong.
        message: SmolStr,
    },

    /// HTTP non-success status from lexicon fetch.
    #[error("HTTP {status} fetching lexicon {nsid}")]
    #[diagnostic(code(jacquard::lexicon::http_error))]
    HttpError {
        /// The NSID of the lexicon being fetched.
        nsid: SmolStr,
        /// The HTTP status code received.
        status: u16,
    },

    /// Required field missing in XRPC response.
    #[error("missing '{field}' field in response for {nsid}")]
    #[diagnostic(
        code(jacquard::lexicon::missing_response_field),
        help("the XRPC response is missing a required field")
    )]
    MissingResponseField {
        /// The NSID of the lexicon being fetched.
        nsid: SmolStr,
        /// Name of the missing field.
        field: &'static str,
    },

    /// The lexicon collection NSID was not valid.
    #[error("invalid collection NSID")]
    #[diagnostic(code(jacquard::lexicon::invalid_collection))]
    InvalidCollection,

    /// The `getRecord` response did not include a CID for the lexicon record.
    #[error("record missing CID for {nsid}")]
    #[diagnostic(code(jacquard::lexicon::missing_cid))]
    MissingCID {
        /// The NSID of the lexicon whose record was missing a CID.
        nsid: SmolStr,
    },

    /// Identity resolution failed while locating the PDS that hosts the lexicon.
    #[error(transparent)]
    #[diagnostic(code(jacquard::lexicon::identity_resolution_failed))]
    IdentityResolution(#[from] crate::resolver::IdentityError),
}

// Implementation on JacquardResolver
impl<C: HttpClient> crate::JacquardResolver<C> {
    /// Resolve lexicon authority via DNS
    ///
    /// Queries `_lexicon.{reversed-authority}` for a TXT record containing `did=...`
    #[cfg(all(feature = "dns", not(target_family = "wasm")))]
    async fn resolve_lexicon_authority_dns<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> std::result::Result<Did, LexiconResolutionError> {
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
                    return Did::new_owned(did_str).map_err(|_| {
                        LexiconResolutionError::invalid_did(authority, did_str)
                            .with_context(format!("resolving NSID {}", nsid))
                    });
                }
            }
        }

        Err(LexiconResolutionError::no_did_found(authority))
    }
}

#[cfg(all(feature = "dns", not(target_family = "wasm")))]
impl<C: HttpClient + Sync> LexiconAuthorityResolver for crate::JacquardResolver<C> {
    async fn resolve_lexicon_authority<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> std::result::Result<Did, LexiconResolutionError> {
        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            let authority = jacquard_common::deps::smol_str::SmolStr::from(nsid.domain_authority());
            if let Some(did) = crate::cache_impl::get(&caches.authority_to_did, &authority) {
                return Ok(did);
            }
        }

        // Resolve via DNS
        let result = self.resolve_lexicon_authority_dns(nsid).await;

        // Cache on success, invalidate on error
        #[cfg(feature = "cache")]
        match &result {
            Ok(did) => {
                if let Some(caches) = &self.caches {
                    let authority =
                        jacquard_common::deps::smol_str::SmolStr::from(nsid.domain_authority());
                    crate::cache_impl::insert(&caches.authority_to_did, authority, did.clone());
                }
            }
            Err(_) => {
                self.invalidate_authority_chain(nsid.domain_authority())
                    .await;
            }
        }

        result
    }
}

#[cfg(not(all(feature = "dns", not(target_family = "wasm"))))]
impl<C: HttpClient + Sync> LexiconAuthorityResolver for crate::JacquardResolver<C> {
    async fn resolve_lexicon_authority<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> std::result::Result<Did, LexiconResolutionError> {
        // Use DNS-over-HTTPS fallback for WASM/non-DNS builds
        self.resolve_lexicon_authority_doh(nsid).await
    }
}

impl<C: HttpClient> crate::JacquardResolver<C> {
    /// Resolve lexicon authority via DNS-over-HTTPS (for WASM compatibility)
    #[allow(dead_code)]
    async fn resolve_lexicon_authority_doh<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> std::result::Result<Did, LexiconResolutionError> {
        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            let authority = jacquard_common::deps::smol_str::SmolStr::from(nsid.domain_authority());
            if let Some(did) = crate::cache_impl::get(&caches.authority_to_did, &authority) {
                return Ok(did);
            }
        }

        let authority = nsid.domain_authority();
        let reversed_authority = authority.split('.').rev().collect::<Vec<_>>().join(".");
        let fqdn = format!("_lexicon.{}.", reversed_authority);

        #[cfg(feature = "tracing")]
        tracing::trace!("resolving lexicon authority via DoH: {}", fqdn);

        let response = self
            .query_dns_doh(&fqdn, "TXT")
            .await
            .map_err(|e| LexiconResolutionError::dns_lookup_failed(authority, e))?;

        // Parse DoH JSON response
        let answers = response
            .get("Answer")
            .and_then(|a| a.as_array())
            .ok_or_else(|| LexiconResolutionError::no_did_found(authority))?;

        for answer in answers {
            if let Some(data) = answer.get("data").and_then(|d| d.as_str()) {
                // TXT records are quoted in DNS responses, strip quotes
                let txt_data = data.trim_matches('"');

                if let Some(did_str) = txt_data.strip_prefix("did=") {
                    let result = Did::new_owned(did_str).map_err(|_| {
                        LexiconResolutionError::invalid_did(authority, did_str)
                            .with_context(format!("resolving NSID {}", nsid))
                    });

                    // Cache on success
                    #[cfg(feature = "cache")]
                    if let Ok(ref did) = result {
                        if let Some(caches) = &self.caches {
                            let authority_key =
                                jacquard_common::deps::smol_str::SmolStr::from(authority);
                            crate::cache_impl::insert(
                                &caches.authority_to_did,
                                authority_key,
                                did.clone(),
                            );
                        }
                    }

                    return result;
                }
            }
        }

        Err(LexiconResolutionError::no_did_found(authority))
    }
}

impl<C: HttpClient + Sync> LexiconSchemaResolver for crate::JacquardResolver<C> {
    async fn resolve_lexicon_schema<S: BosStr + Sync>(
        &self,
        nsid: &Nsid<S>,
    ) -> std::result::Result<ResolvedLexiconSchema<'static>, LexiconResolutionError> {
        use jacquard_common::xrpc::atproto::GetRecord;
        use jacquard_common::{IntoStatic, xrpc::XrpcExt};

        let nsid_str = nsid.as_str();
        let owned_nsid: Nsid = Nsid::new_owned(nsid_str).expect("already validated NSID");

        // Try cache first
        #[cfg(feature = "cache")]
        if let Some(caches) = &self.caches {
            if let Some(schema) = crate::cache_impl::get(&caches.nsid_to_schema, &owned_nsid) {
                return Ok((*schema).clone());
            }
        }

        // Perform resolution
        let result = async {
            // 1. Resolve authority DID via DNS
            let authority_did = self.resolve_lexicon_authority(nsid).await?;

            #[cfg(feature = "tracing")]
            tracing::trace!(
                "resolved lexicon authority {} -> {}",
                nsid.domain_authority(),
                authority_did
            );

            // 2. Resolve DID document to get PDS endpoint
            let did_doc_resp = self.resolve_did_doc(&authority_did).await?;
            let did_doc = did_doc_resp.parse()?;
            let pds = did_doc
                .pds_endpoint()
                .ok_or_else(|| IdentityError::missing_pds_endpoint(authority_did.as_str()))?;

            #[cfg(feature = "tracing")]
            tracing::trace!("fetching lexicon {} from PDS {}", nsid, pds);

            // 3. Fetch lexicon record via XRPC getRecord
            let collection = Nsid::new_owned("com.atproto.lexicon.schema")
                .map_err(|_| LexiconResolutionError::invalid_collection())?;

            let request = GetRecord {
                repo: AtIdentifier::Did(authority_did.clone()),
                collection,
                rkey: RecordKey::any_owned(nsid_str).unwrap(),
                cid: None,
            };

            let response = self
                .xrpc(pds)
                .send(&request)
                .await
                .map_err(|e| LexiconResolutionError::fetch_failed(nsid_str, e))?;

            let output = response
                .into_output()
                .map_err(|e| LexiconResolutionError::fetch_failed(nsid_str, e))?;

            // 4. Parse lexicon document from value
            let json_str = serde_json::to_string(&output.value)
                .map_err(|e| LexiconResolutionError::parse_failed(nsid_str, e))?;

            let doc: jacquard_lexicon::lexicon::LexiconDoc = serde_json::from_str(&json_str)
                .map_err(|e| LexiconResolutionError::parse_failed(nsid_str, e))?;

            #[cfg(feature = "tracing")]
            tracing::trace!("successfully parsed lexicon schema {}", nsid);

            let cid = output
                .cid
                .ok_or_else(|| LexiconResolutionError::missing_cid(nsid_str))?;

            Ok(ResolvedLexiconSchema {
                nsid: owned_nsid.clone(),
                repo: authority_did,
                cid,
                doc: doc.into_static(),
            })
        }
        .await;

        // Handle result
        match result {
            Ok(schema) => {
                // Cache successful resolution
                #[cfg(feature = "cache")]
                if let Some(caches) = &self.caches {
                    crate::cache_impl::insert(
                        &caches.nsid_to_schema,
                        owned_nsid,
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
