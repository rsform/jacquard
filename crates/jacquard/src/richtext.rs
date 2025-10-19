//! Rich text utilities for Bluesky posts
//!
//! Provides parsing and building of rich text with facets (mentions, links, tags)
//! and detection of embed candidates (record and external embeds).

#[cfg(feature = "api_bluesky")]
use crate::api::app_bsky::richtext::facet::Facet;
use crate::common::CowStr;
use jacquard_common::IntoStatic;
use jacquard_common::types::did::{DID_REGEX, Did};
use jacquard_common::types::handle::HANDLE_REGEX;
use regex::Regex;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::LazyLock;

// Regex patterns based on Bluesky's official implementation
// https://github.com/bluesky-social/atproto/blob/main/packages/api/src/rich-text/util.ts

static MENTION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|\s|\()(@)([a-zA-Z0-9.:-]+)(\b)").unwrap());

static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|\s|\()((https?://[\S]+)|((?<domain>[a-z][a-z0-9]*(\.[a-z0-9]+)+)[\S]*))")
        .unwrap()
});

static TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Simplified version - full unicode handling would need more work
    Regex::new(r"(^|\s)[#＃]([^\s\x{00AD}\x{2060}\x{200A}\x{200B}\x{200C}\x{200D}]+)").unwrap()
});

static MARKDOWN_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());

static TRAILING_PUNCT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{P}+$").unwrap());

// Sanitization regex - removes soft hyphens, zero-width chars, normalizes newlines
// Matches one of the special chars, optionally followed by whitespace, repeated
// This ensures at least one special char is in the match (won't match pure spaces)
static SANITIZE_NEWLINES_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([\r\n\u{00AD}\u{2060}\u{200D}\u{200C}\u{200B}]\s*)+").unwrap());

/// Default domains that support at-URI extraction from URLs
/// (bsky.app URL patterns like /profile/{actor}/post/{rkey})
#[cfg(feature = "api_bluesky")]
static DEFAULT_EMBED_DOMAINS: &[&str] = &[
    "bsky.app",
    "deer.social",
    "blacksky.community",
    "catsky.social",
];

/// Marker type indicating all facets are resolved (no handles pending DID resolution)
pub struct Resolved;

/// Marker type indicating some facets may need resolution (handles → DIDs)
pub struct Unresolved;

/// Rich text with facets (mentions, links, tags)
#[derive(Debug, Clone)]
#[cfg(feature = "api_bluesky")]
pub struct RichText<'a> {
    /// The text content
    pub text: CowStr<'a>,
    /// Facets (mentions, links, tags)
    pub facets: Option<Vec<Facet<'a>>>,
}

#[cfg(feature = "api_bluesky")]
impl RichText<'static> {
    /// Entry point for parsing text with automatic facet detection
    ///
    /// Uses default embed domains (bsky.app, deer.social) for at-URI extraction.
    pub fn parse(text: impl AsRef<str>) -> RichTextBuilder<Unresolved> {
        parse(text)
    }

    /// Entry point for manual richtext construction
    pub fn builder() -> RichTextBuilder<Resolved> {
        RichTextBuilder::builder()
    }
}

/// Detected embed candidate from URL or at-URI
#[derive(Debug, Clone)]
#[cfg(feature = "api_bluesky")]
pub enum EmbedCandidate<'a> {
    /// Bluesky record (post, list, starterpack, feed)
    Record {
        /// The at:// URI identifying the record
        at_uri: crate::types::aturi::AtUri<'a>,
        /// Strong reference (repo + CID) if resolved
        strong_ref: Option<crate::api::com_atproto::repo::strong_ref::StrongRef<'a>>,
    },
    /// External link embed
    External {
        /// The URL
        url: CowStr<'a>,
        /// OpenGraph metadata if fetched
        metadata: Option<ExternalMetadata<'a>>,
    },
}

/// External embed metadata (OpenGraph)
#[derive(Debug, Clone)]
#[cfg(feature = "api_bluesky")]
pub struct ExternalMetadata<'a> {
    /// Page title
    pub title: CowStr<'a>,
    /// Page description
    pub description: CowStr<'a>,
    /// Thumbnail URL
    pub thumbnail: Option<CowStr<'a>>,
}

/// Rich text builder supporting both parsing and manual construction
#[derive(Debug)]
pub struct RichTextBuilder<State> {
    text: String,
    facet_candidates: Vec<FacetCandidate>,
    #[cfg(feature = "api_bluesky")]
    embed_candidates: Option<Vec<EmbedCandidate<'static>>>,
    _state: PhantomData<State>,
}

/// Internal representation of facet before resolution
///
/// Stores minimal data to save memory:
/// - Markdown links store URL (since syntax is stripped from text)
/// - Mentions/tags store just ranges (@ and # included, extract at build time)
/// - Links store just ranges (normalize URL at build time)
#[derive(Debug, Clone)]
enum FacetCandidate {
    /// Markdown link: `[display](url)` → display text in final text
    MarkdownLink {
        /// Range of display text in final processed text
        display_range: Range<usize>,
        /// URL from markdown (not in final text, so must store)
        url: String,
    },
    /// Mention: `@handle.bsky.social`
    /// Range includes the @ symbol, process at build time
    Mention {
        /// Range in text including @ symbol
        range: Range<usize>,
        /// DID when provided, otherwise resolved later
        did: Option<Did<'static>>,
    },
    /// Plain URL link
    /// Range points to URL in text, normalize at build time
    Link {
        /// Range in text pointing to URL (may need normalization)
        range: Range<usize>,
    },
    /// Hashtag: `#tag`
    /// Range includes the # symbol, process at build time
    Tag {
        /// Range in text including # symbol
        range: Range<usize>,
    },
}

/// Sanitize text by removing invisible characters and normalizing newlines
///
/// This removes:
/// - Soft hyphens (\u{00AD})
/// - Zero-width non-joiner (\u{200C})
/// - Zero-width joiner (\u{200D})
/// - Zero-width space (\u{200B})
/// - Word joiner (\u{2060})
///
/// And normalizes all newline variants (\r\n, \r, \n) to \n, while collapsing
/// runs of newlines and invisible chars to at most two newlines.
fn sanitize_text(text: &str) -> String {
    SANITIZE_NEWLINES_REGEX
        .replace_all(text, |caps: &regex::Captures| {
            let matched = caps.get(0).unwrap().as_str();

            // Count newline sequences, treating \r\n as one unit
            let mut newline_sequences = 0;
            let mut chars = matched.chars().peekable();

            while let Some(c) = chars.next() {
                if c == '\r' {
                    // Check if followed by \n
                    if chars.peek() == Some(&'\n') {
                        chars.next(); // consume the \n
                    }
                    newline_sequences += 1;
                } else if c == '\n' {
                    newline_sequences += 1;
                }
                // Skip invisible chars (they don't increment count)
            }

            if newline_sequences == 0 {
                // Only invisible chars, remove them
                ""
            } else if newline_sequences == 1 {
                "\n"
            } else {
                // Multiple newlines, collapse to \n\n (paragraph break)
                "\n\n"
            }
        })
        .to_string()
}

/// Entry point for parsing text with automatic facet detection
///
/// Uses default embed domains (bsky.app, deer.social) for at-URI extraction.
/// For custom domains, use [`parse_with_domains`].
pub fn parse(text: impl AsRef<str>) -> RichTextBuilder<Unresolved> {
    #[cfg(feature = "api_bluesky")]
    {
        parse_with_domains(text, DEFAULT_EMBED_DOMAINS)
    }
    #[cfg(not(feature = "api_bluesky"))]
    {
        parse_with_domains(text, &[])
    }
}

/// Parse text with custom embed domains for at-URI extraction
///
/// This allows specifying additional domains (beyond bsky.app and deer.social)
/// that use the same URL patterns for records (e.g., /profile/{actor}/post/{rkey}).
#[cfg(feature = "api_bluesky")]
pub fn parse_with_domains(
    text: impl AsRef<str>,
    embed_domains: &[&str],
) -> RichTextBuilder<Unresolved> {
    // Step 0: Sanitize text (remove invisible chars, normalize newlines)
    let text = sanitize_text(text.as_ref());

    let mut facet_candidates = Vec::new();
    let mut embed_candidates = Vec::new();

    // Step 1: Detect and strip markdown links first
    let (text_processed, markdown_facets) = detect_markdown_links(&text);

    // Check markdown links for embed candidates
    for facet in &markdown_facets {
        if let FacetCandidate::MarkdownLink { url, .. } = facet {
            if let Some(embed) = classify_embed(url, embed_domains) {
                embed_candidates.push(embed);
            }
        }
    }

    facet_candidates.extend(markdown_facets);

    // Step 2: Detect mentions
    let mention_facets = detect_mentions(&text_processed);
    facet_candidates.extend(mention_facets);

    // Step 3: Detect URLs
    let url_facets = detect_urls(&text_processed);

    // Check URLs for embed candidates
    for facet in &url_facets {
        if let FacetCandidate::Link { range } = facet {
            let url = &text_processed[range.clone()];
            if let Some(embed) = classify_embed(url, embed_domains) {
                embed_candidates.push(embed);
            }
        }
    }

    facet_candidates.extend(url_facets);

    // Step 4: Detect tags
    let tag_facets = detect_tags(&text_processed);
    facet_candidates.extend(tag_facets);

    RichTextBuilder {
        text: text_processed,
        facet_candidates,
        embed_candidates: if embed_candidates.is_empty() {
            None
        } else {
            Some(embed_candidates)
        },
        _state: PhantomData,
    }
}

/// Parse text without embed detection (no api_bluesky feature)
#[cfg(not(feature = "api_bluesky"))]
pub fn parse_with_domains(
    text: impl AsRef<str>,
    _embed_domains: &[&str],
) -> RichTextBuilder<Unresolved> {
    // Step 0: Sanitize text (remove invisible chars, normalize newlines)
    let text = sanitize_text(text.as_ref());

    let mut facet_candidates = Vec::new();

    // Step 1: Detect and strip markdown links first
    let (text_processed, markdown_facets) = detect_markdown_links(&text);
    facet_candidates.extend(markdown_facets);

    // Step 2: Detect mentions
    let mention_facets = detect_mentions(&text_processed);
    facet_candidates.extend(mention_facets);

    // Step 3: Detect URLs
    let url_facets = detect_urls(&text_processed);
    facet_candidates.extend(url_facets);

    // Step 4: Detect tags
    let tag_facets = detect_tags(&text_processed);
    facet_candidates.extend(tag_facets);

    RichTextBuilder {
        text: text_processed,
        facet_candidates,
        _state: PhantomData,
    }
}

impl RichTextBuilder<Resolved> {
    /// Entry point for manual richtext construction
    pub fn builder() -> Self {
        RichTextBuilder {
            text: String::new(),
            facet_candidates: Vec::new(),
            #[cfg(feature = "api_bluesky")]
            embed_candidates: None,
            _state: PhantomData,
        }
    }

    /// Add a mention by handle (transitions to Unresolved state)
    pub fn mention_handle(
        mut self,
        handle: impl AsRef<str>,
        range: Option<Range<usize>>,
    ) -> RichTextBuilder<Unresolved> {
        let handle = handle.as_ref();
        let range = range.unwrap_or_else(|| {
            // Scan text for @handle
            let search = format!("@{}", handle);
            self.find_substring(&search).unwrap_or(0..0)
        });

        self.facet_candidates
            .push(FacetCandidate::Mention { range, did: None });

        RichTextBuilder {
            text: self.text,
            facet_candidates: self.facet_candidates,
            #[cfg(feature = "api_bluesky")]
            embed_candidates: self.embed_candidates,
            _state: PhantomData,
        }
    }
}

impl<S> RichTextBuilder<S> {
    /// Set the text content
    pub fn text(mut self, text: impl AsRef<str>) -> Self {
        self.text = sanitize_text(text.as_ref());
        self
    }

    /// Add a mention facet with a resolved DID (requires explicit range)
    pub fn mention(mut self, did: &crate::types::did::Did<'_>, range: Range<usize>) -> Self {
        self.facet_candidates.push(FacetCandidate::Mention {
            range,
            did: Some(did.clone().into_static()),
        });
        self
    }

    /// Add a link facet (auto-detects range if None)
    pub fn link(mut self, url: impl AsRef<str>, range: Option<Range<usize>>) -> Self {
        let url = url.as_ref();
        let range = range.unwrap_or_else(|| {
            // Scan text for the URL
            self.find_substring(url).unwrap_or(0..0)
        });

        self.facet_candidates.push(FacetCandidate::Link { range });
        self
    }

    /// Add a tag facet (auto-detects range if None)
    pub fn tag(mut self, tag: impl AsRef<str>, range: Option<Range<usize>>) -> Self {
        let tag = tag.as_ref();
        let range = range.unwrap_or_else(|| {
            // Scan text for #tag
            let search = format!("#{}", tag);
            self.find_substring(&search).unwrap_or(0..0)
        });

        self.facet_candidates.push(FacetCandidate::Tag { range });
        self
    }

    /// Add a markdown-style link with display text
    pub fn markdown_link(mut self, url: impl Into<String>, display_range: Range<usize>) -> Self {
        self.facet_candidates.push(FacetCandidate::MarkdownLink {
            url: url.into(),
            display_range,
        });
        self
    }

    #[cfg(feature = "api_bluesky")]
    /// Add a record embed candidate
    pub fn embed_record(
        mut self,
        at_uri: crate::types::aturi::AtUri<'static>,
        strong_ref: Option<crate::api::com_atproto::repo::strong_ref::StrongRef<'static>>,
    ) -> Self {
        self.embed_candidates
            .get_or_insert_with(Vec::new)
            .push(EmbedCandidate::Record { at_uri, strong_ref });
        self
    }

    #[cfg(feature = "api_bluesky")]
    /// Add an external embed candidate
    pub fn embed_external(
        mut self,
        url: impl Into<CowStr<'static>>,
        metadata: Option<ExternalMetadata<'static>>,
    ) -> Self {
        self.embed_candidates
            .get_or_insert_with(Vec::new)
            .push(EmbedCandidate::External {
                url: url.into(),
                metadata,
            });
        self
    }

    fn find_substring(&self, needle: &str) -> Option<Range<usize>> {
        self.text.find(needle).map(|start| {
            let end = start + needle.len();
            start..end
        })
    }
}

fn detect_markdown_links(text: &str) -> (String, Vec<FacetCandidate>) {
    let mut result = String::with_capacity(text.len());
    let mut facets = Vec::new();
    let mut last_end = 0;
    let mut offset = 0;

    for cap in MARKDOWN_LINK_REGEX.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let display_text = cap.get(1).unwrap().as_str();
        let url = cap.get(2).unwrap().as_str();

        // Append text before this match
        result.push_str(&text[last_end..full_match.start()]);

        // Append only the display text (strip markdown syntax)
        let start = result.len() - offset;
        result.push_str(display_text);
        let end = result.len() - offset;

        // Track offset change (we removed the markdown syntax)
        offset += full_match.as_str().len() - display_text.len();

        // Store URL string since it's not in the final text
        facets.push(FacetCandidate::MarkdownLink {
            display_range: start..end,
            url: url.to_string(),
        });

        last_end = full_match.end();
    }

    // Append remaining text
    result.push_str(&text[last_end..]);

    (result, facets)
}

fn detect_mentions(text: &str) -> Vec<FacetCandidate> {
    let mut facets = Vec::new();

    for cap in MENTION_REGEX.captures_iter(text) {
        let handle = cap.get(3).unwrap().as_str();

        if !HANDLE_REGEX.is_match(handle) && !DID_REGEX.is_match(handle) {
            continue;
        }

        let did = if let Ok(did) = Did::new(handle) {
            Some(did.into_static())
        } else {
            None
        };

        // Store range including @ symbol - extract text at build time
        let at_sign = cap.get(2).unwrap();
        let start = at_sign.start();
        let end = cap.get(3).unwrap().end();

        facets.push(FacetCandidate::Mention {
            range: start..end,
            did,
        });
    }

    facets
}

fn detect_urls(text: &str) -> Vec<FacetCandidate> {
    let mut facets = Vec::new();

    for cap in URL_REGEX.captures_iter(text) {
        let url_match = if let Some(full_url) = cap.get(3) {
            full_url
        } else if let Some(_domain) = cap.name("domain") {
            // Bare domain - will prepend https:// at build time
            cap.get(2).unwrap()
        } else {
            continue;
        };

        let url_str = url_match.as_str();

        // Calculate actual end after stripping trailing punctuation
        let trimmed_len = if let Some(trimmed) = TRAILING_PUNCT_REGEX.find(url_str) {
            trimmed.start()
        } else {
            url_str.len()
        };

        if trimmed_len == 0 {
            continue;
        }

        let start = url_match.start();
        let end = start + trimmed_len;

        // Store just the range - normalize URL at build time
        facets.push(FacetCandidate::Link { range: start..end });
    }

    facets
}

fn detect_tags(text: &str) -> Vec<FacetCandidate> {
    let mut facets = Vec::new();

    for cap in TAG_REGEX.captures_iter(text) {
        let tag_match = cap.get(2).unwrap();
        let tag_str = tag_match.as_str();

        // Calculate trimmed length after stripping trailing punctuation
        let trimmed_len = if let Some(trimmed) = TRAILING_PUNCT_REGEX.find(tag_str) {
            trimmed.start()
        } else {
            tag_str.len()
        };

        // Validate length (0-64 chars per Bluesky spec)
        if trimmed_len == 0 || trimmed_len > 64 {
            continue;
        }

        let hash_pos = cap.get(0).unwrap().start();
        // Find the actual # character position
        let hash_start = text[hash_pos..]
            .chars()
            .position(|c| c == '#' || c == '＃')
            .unwrap();
        let start = hash_pos + hash_start;
        let end = start + 1 + trimmed_len; // # + tag length

        // Store range including # symbol - extract and process at build time
        facets.push(FacetCandidate::Tag { range: start..end });
    }

    facets
}

/// Classifies a URL or at-URI as an embed candidate
#[cfg(feature = "api_bluesky")]
fn classify_embed(url: &str, embed_domains: &[&str]) -> Option<EmbedCandidate<'static>> {
    use crate::types::aturi::AtUri;

    // Check if it's an at:// URI
    if url.starts_with("at://") {
        if let Ok(at_uri) = AtUri::new(url) {
            return Some(EmbedCandidate::Record {
                at_uri: at_uri.into_static(),
                strong_ref: None,
            });
        }
    }

    // Check if it's an HTTP(S) URL
    if url.starts_with("http://") || url.starts_with("https://") {
        // Try to extract at-uri from configured domain URL patterns
        if let Some(at_uri) = extract_at_uri_from_url(url, embed_domains) {
            return Some(EmbedCandidate::Record {
                at_uri,
                strong_ref: None,
            });
        }

        // Otherwise, it's an external embed
        return Some(EmbedCandidate::External {
            url: CowStr::from(url.to_string()),
            metadata: None,
        });
    }

    None
}

/// Extracts an at-URI from a URL with bsky.app-style path patterns
///
/// Supports these patterns:
/// - https://{domain}/profile/{handle|did}/post/{rkey} → at://{actor}/app.bsky.feed.post/{rkey}
/// - https://{domain}/profile/{handle|did}/lists/{rkey} → at://{actor}/app.bsky.graph.list/{rkey}
/// - https://{domain}/profile/{handle|did}/feed/{rkey} → at://{actor}/app.bsky.feed.generator/{rkey}
/// - https://{domain}/starter-pack/{handle|did}/{rkey} → at://{actor}/app.bsky.graph.starterpack/{rkey}
/// - https://{domain}/profile/{handle|did}/{collection}/{rkey} → at://{actor}/{collection}/{rkey} (if collection looks like NSID)
///
/// Only works for domains in the provided `embed_domains` list.
#[cfg(feature = "api_bluesky")]
fn extract_at_uri_from_url(
    url: &str,
    embed_domains: &[&str],
) -> Option<crate::types::aturi::AtUri<'static>> {
    use crate::types::aturi::AtUri;

    // Parse URL
    let url_parsed = url::Url::parse(url).ok()?;

    // Check if domain is in allowed list
    let domain = url_parsed.domain()?;
    if !embed_domains.contains(&domain) {
        return None;
    }

    let path = url_parsed.path();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let at_uri_str = match segments.as_slice() {
        // Known shortcuts
        ["profile", actor, "post", rkey] => {
            format!("at://{}/app.bsky.feed.post/{}", actor, rkey)
        }
        ["profile", actor, "lists", rkey] => {
            format!("at://{}/app.bsky.graph.list/{}", actor, rkey)
        }
        ["profile", actor, "feed", rkey] => {
            format!("at://{}/app.bsky.feed.generator/{}", actor, rkey)
        }
        ["starter-pack", actor, rkey] => {
            format!("at://{}/app.bsky.graph.starterpack/{}", actor, rkey)
        }
        // Generic pattern: /profile/{actor}/{collection}/{rkey}
        // Accept if collection looks like it could be an NSID (contains dots)
        ["profile", actor, collection, rkey] if collection.contains('.') => {
            format!("at://{}/{}/{}", actor, collection, rkey)
        }
        _ => return None,
    };

    AtUri::new(&at_uri_str).ok().map(|u| u.into_static())
}

use jacquard_common::types::string::AtStrError;
use thiserror::Error;

/// Errors that can occur during richtext building
#[derive(Debug, Error)]
pub enum RichTextError {
    /// Handle found that needs resolution but no resolver provided
    #[error("Handle '{0}' requires resolution - use build_async() with an IdentityResolver")]
    HandleNeedsResolution(String),

    /// Facets overlap (not allowed by spec)
    #[error("Facets overlap at byte range {0}..{1}")]
    OverlappingFacets(usize, usize),

    /// Identity resolution failed
    #[error("Failed to resolve identity")]
    IdentityResolution(#[from] jacquard_identity::resolver::IdentityError),

    /// Invalid byte range
    #[error("Invalid byte range {start}..{end} for text of length {text_len}")]
    InvalidRange {
        /// Range start position
        start: usize,
        /// Range end position
        end: usize,
        /// Total text length
        text_len: usize,
    },

    /// Invalid AT Protocol string (URI, DID, or Handle)
    #[error("Invalid AT Protocol string")]
    InvalidAtStr(#[from] AtStrError),

    /// Invalid URI
    #[error("Invalid URI")]
    Uri(#[from] jacquard_common::types::uri::UriParseError),
}

#[cfg(feature = "api_bluesky")]
impl RichTextBuilder<Resolved> {
    /// Build the richtext (sync - all facets must be resolved)
    pub fn build(self) -> Result<RichText<'static>, RichTextError> {
        use std::collections::BTreeMap;
        if self.facet_candidates.is_empty() {
            return Ok(RichText {
                text: CowStr::from(self.text),
                facets: None,
            });
        }

        // Sort facets by start position
        let mut candidates = self.facet_candidates;
        candidates.sort_by_key(|fc| match fc {
            FacetCandidate::MarkdownLink { display_range, .. } => display_range.start,
            FacetCandidate::Mention { range, .. } => range.start,
            FacetCandidate::Link { range } => range.start,
            FacetCandidate::Tag { range } => range.start,
        });

        // Check for overlaps and convert to Facet types
        let mut facets = Vec::with_capacity(candidates.len());
        let mut last_end = 0;
        let text_len = self.text.len();

        for candidate in candidates {
            use crate::api::app_bsky::richtext::facet::{ByteSlice, Facet};

            let (range, feature) = match candidate {
                FacetCandidate::MarkdownLink { display_range, url } => {
                    // MarkdownLink stores URL directly, use display_range for index

                    let feature = crate::api::app_bsky::richtext::facet::FacetFeaturesItem::Link(
                        Box::new(crate::api::app_bsky::richtext::facet::Link {
                            uri: crate::types::uri::Uri::new_owned(&url)?,
                            extra_data: BTreeMap::new(),
                        }),
                    );
                    (display_range, feature)
                }
                FacetCandidate::Mention { range, did } => {
                    // In Resolved state, DID must be present
                    let did = did.ok_or_else(|| {
                        // Extract handle from text for error message
                        let handle = if range.end <= text_len {
                            self.text[range.clone()].trim_start_matches('@')
                        } else {
                            "<invalid range>"
                        };
                        RichTextError::HandleNeedsResolution(handle.to_string())
                    })?;

                    let feature = crate::api::app_bsky::richtext::facet::FacetFeaturesItem::Mention(
                        Box::new(crate::api::app_bsky::richtext::facet::Mention {
                            did,
                            extra_data: BTreeMap::new(),
                        }),
                    );
                    (range, feature)
                }
                FacetCandidate::Link { range } => {
                    // Extract URL from text[range] and normalize
                    if range.end > text_len {
                        return Err(RichTextError::InvalidRange {
                            start: range.start,
                            end: range.end,
                            text_len,
                        });
                    }

                    let mut url = self.text[range.clone()].to_string();

                    // Prepend https:// if URL doesn't have a scheme
                    if !url.starts_with("http://") && !url.starts_with("https://") {
                        url = format!("https://{}", url);
                    }

                    let feature = crate::api::app_bsky::richtext::facet::FacetFeaturesItem::Link(
                        Box::new(crate::api::app_bsky::richtext::facet::Link {
                            uri: crate::types::uri::Uri::new_owned(&url)?,
                            extra_data: BTreeMap::new(),
                        }),
                    );
                    (range, feature)
                }
                FacetCandidate::Tag { range } => {
                    // Extract tag from text[range] (includes #), strip # and trailing punct

                    use smol_str::ToSmolStr;
                    if range.end > text_len {
                        return Err(RichTextError::InvalidRange {
                            start: range.start,
                            end: range.end,
                            text_len,
                        });
                    }

                    let tag_with_hash = &self.text[range.clone()];
                    // Strip # prefix (could be # or ＃)
                    let tag = tag_with_hash
                        .trim_start_matches('#')
                        .trim_start_matches('＃');

                    let feature = crate::api::app_bsky::richtext::facet::FacetFeaturesItem::Tag(
                        Box::new(crate::api::app_bsky::richtext::facet::Tag {
                            tag: CowStr::from(tag.to_smolstr()),
                            extra_data: BTreeMap::new(),
                        }),
                    );
                    (range, feature)
                }
            };

            // Check overlap
            if range.start < last_end {
                return Err(RichTextError::OverlappingFacets(range.start, range.end));
            }

            // Validate range
            if range.end > text_len {
                return Err(RichTextError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    text_len,
                });
            }

            facets.push(Facet {
                index: ByteSlice {
                    byte_start: range.start as i64,
                    byte_end: range.end as i64,
                    extra_data: BTreeMap::new(),
                },
                features: vec![feature],
                extra_data: BTreeMap::new(),
            });

            last_end = range.end;
        }

        Ok(RichText {
            text: CowStr::from(self.text),
            facets: Some(facets.into_static()),
        })
    }
}

#[cfg(feature = "api_bluesky")]
impl RichTextBuilder<Unresolved> {
    /// Build richtext, resolving handles to DIDs using the provided resolver
    pub async fn build_async<R>(self, resolver: &R) -> Result<RichText<'static>, RichTextError>
    where
        R: jacquard_identity::resolver::IdentityResolver + Sync,
    {
        use crate::api::app_bsky::richtext::facet::{
            ByteSlice, FacetFeaturesItem, Link, Mention, Tag,
        };
        use std::collections::BTreeMap;

        if self.facet_candidates.is_empty() {
            return Ok(RichText {
                text: CowStr::from(self.text),
                facets: None,
            });
        }

        // Sort facets by start position
        let mut candidates = self.facet_candidates;
        candidates.sort_by_key(|fc| match fc {
            FacetCandidate::MarkdownLink { display_range, .. } => display_range.start,
            FacetCandidate::Mention { range, .. } => range.start,
            FacetCandidate::Link { range } => range.start,
            FacetCandidate::Tag { range } => range.start,
        });

        // Resolve handles and convert to Facet types
        let mut facets = Vec::with_capacity(candidates.len());
        let mut last_end = 0;
        let text_len = self.text.len();

        for candidate in candidates {
            let (range, feature) = match candidate {
                FacetCandidate::MarkdownLink { display_range, url } => {
                    // MarkdownLink stores URL directly, use display_range for index

                    let feature = FacetFeaturesItem::Link(Box::new(Link {
                        uri: crate::types::uri::Uri::new_owned(&url)?,
                        extra_data: BTreeMap::new(),
                    }));
                    (display_range, feature)
                }
                FacetCandidate::Mention { range, did } => {
                    let did = if let Some(did) = did {
                        // Already resolved
                        did
                    } else {
                        // Extract handle from text and resolve
                        if range.end > text_len {
                            return Err(RichTextError::InvalidRange {
                                start: range.start,
                                end: range.end,
                                text_len,
                            });
                        }

                        let handle_str = self.text[range.clone()].trim_start_matches('@');
                        let handle = jacquard_common::types::handle::Handle::new(handle_str)?;

                        resolver.resolve_handle(&handle).await?
                    };

                    let feature = FacetFeaturesItem::Mention(Box::new(Mention {
                        did,
                        extra_data: BTreeMap::new(),
                    }));
                    (range, feature)
                }
                FacetCandidate::Link { range } => {
                    // Extract URL from text[range] and normalize

                    if range.end > text_len {
                        return Err(RichTextError::InvalidRange {
                            start: range.start,
                            end: range.end,
                            text_len,
                        });
                    }

                    let mut url = self.text[range.clone()].to_string();

                    // Prepend https:// if URL doesn't have a scheme
                    if !url.starts_with("http://") && !url.starts_with("https://") {
                        url = format!("https://{}", url);
                    }

                    let feature = FacetFeaturesItem::Link(Box::new(Link {
                        uri: crate::types::uri::Uri::new_owned(&url)?,
                        extra_data: BTreeMap::new(),
                    }));
                    (range, feature)
                }
                FacetCandidate::Tag { range } => {
                    // Extract tag from text[range] (includes #), strip # and trailing punct

                    use smol_str::ToSmolStr;
                    if range.end > text_len {
                        return Err(RichTextError::InvalidRange {
                            start: range.start,
                            end: range.end,
                            text_len,
                        });
                    }

                    let tag_with_hash = &self.text[range.clone()];
                    // Strip # prefix (could be # or ＃)
                    let tag = tag_with_hash
                        .trim_start_matches('#')
                        .trim_start_matches('＃');

                    let feature = FacetFeaturesItem::Tag(Box::new(Tag {
                        tag: CowStr::from(tag.to_smolstr()),
                        extra_data: BTreeMap::new(),
                    }));
                    (range, feature)
                }
            };

            // Check overlap
            if range.start < last_end {
                return Err(RichTextError::OverlappingFacets(range.start, range.end));
            }

            // Validate range
            if range.end > text_len {
                return Err(RichTextError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    text_len,
                });
            }

            facets.push(Facet {
                index: ByteSlice {
                    byte_start: range.start as i64,
                    byte_end: range.end as i64,
                    extra_data: BTreeMap::new(),
                },
                features: vec![feature],
                extra_data: BTreeMap::new(),
            });

            last_end = range.end;
        }

        Ok(RichText {
            text: CowStr::from(self.text),
            facets: Some(facets.into_static()),
        })
    }

    /// Build richtext with embed resolution using HttpClient
    ///
    /// This resolves handles to DIDs and fetches OpenGraph metadata for external links.
    pub async fn build_with_embeds_async<C>(
        mut self,
        client: &C,
    ) -> Result<(RichText<'static>, Option<Vec<EmbedCandidate<'static>>>), RichTextError>
    where
        C: jacquard_common::http_client::HttpClient
            + jacquard_identity::resolver::IdentityResolver
            + Sync,
    {
        // Extract embed candidates
        let embed_candidates = self.embed_candidates.take().unwrap_or_default();

        // Build facets (resolves handles)
        let richtext = self.build_async(client).await?;

        // Now resolve embed candidates
        let mut resolved_embeds = Vec::new();

        for candidate in embed_candidates {
            match candidate {
                EmbedCandidate::Record { at_uri, strong_ref } => {
                    // TODO: could fetch the record to get CID for strong_ref
                    // For now, just pass through
                    resolved_embeds.push(EmbedCandidate::Record { at_uri, strong_ref });
                }
                EmbedCandidate::External {
                    url,
                    metadata: None,
                } => {
                    // Fetch OpenGraph metadata
                    match fetch_opengraph_metadata(client, &url).await {
                        Ok(Some(metadata)) => {
                            resolved_embeds.push(EmbedCandidate::External {
                                url,
                                metadata: Some(metadata),
                            });
                        }
                        Ok(None) | Err(_) => {
                            // If we fail to fetch metadata, include embed without metadata
                            resolved_embeds.push(EmbedCandidate::External {
                                url,
                                metadata: None,
                            });
                        }
                    }
                }
                other => resolved_embeds.push(other),
            }
        }

        Ok((richtext, Some(resolved_embeds).filter(|v| !v.is_empty())))
    }
}

/// Fetch OpenGraph metadata from a URL using the webpage crate
#[cfg(feature = "api_bluesky")]
async fn fetch_opengraph_metadata<C>(
    client: &C,
    url: &str,
) -> Result<Option<ExternalMetadata<'static>>, Box<dyn std::error::Error + Send + Sync>>
where
    C: jacquard_common::http_client::HttpClient,
{
    // Build HTTP GET request
    let request = http::Request::builder()
        .method("GET")
        .uri(url)
        .header("User-Agent", "jacquard/0.6")
        .body(Vec::new())
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Fetch the page
    let response = client
        .send_http(request)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Parse HTML body
    let html = String::from_utf8_lossy(response.body());

    // Use webpage crate to extract OpenGraph metadata
    let info = webpage::HTML::from_string(html.to_string(), Some(url.to_string()))
        .ok()
        .map(|html| html.opengraph);

    if let Some(og) = info {
        // Extract title, description, and thumbnail

        use jacquard_common::cowstr::ToCowStr;
        let title = og.properties.get("title").map(|s| s.to_cowstr());

        let description = og.properties.get("description").map(|s| s.to_cowstr());

        let thumbnail = og.images.first().map(|img| CowStr::from(img.url.clone()));

        // Only return metadata if we have at least a title
        if let Some(title) = title {
            return Ok(Some(ExternalMetadata {
                title: title.into_static(),
                description: description
                    .unwrap_or_else(|| CowStr::new_static(""))
                    .into_static(),
                thumbnail: thumbnail.into_static(),
            }));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests;
