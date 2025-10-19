//! Rich text utilities for Bluesky posts
//!
//! Provides parsing and building of rich text with facets (mentions, links, tags)
//! and detection of embed candidates (record and external embeds).

use crate::common::CowStr;
use regex::Regex;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::LazyLock;

// Regex patterns based on Bluesky's official implementation
// https://github.com/bluesky-social/atproto/blob/main/packages/api/src/rich-text/util.ts

static MENTION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|\s|\()(@)([a-zA-Z0-9.-]+)(\b)").unwrap());

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

/// Marker type indicating all facets are resolved (no handles pending DID resolution)
pub struct Resolved;

/// Marker type indicating some facets may need resolution (handles → DIDs)
pub struct Unresolved;

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

/// Entry point for parsing text with automatic facet detection
pub fn parse(text: impl Into<String>) -> RichTextBuilder<Unresolved> {
    let text = text.into();
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
        #[cfg(feature = "api_bluesky")]
        embed_candidates: Vec::new(),
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
            embed_candidates: Vec::new(),
            _state: PhantomData,
        }
    }

    /// Add a mention by handle (transitions to Unresolved state)
    pub fn mention_handle(
        self,
        handle: impl AsRef<str>,
        range: Option<Range<usize>>,
    ) -> RichTextBuilder<Unresolved> {
        let handle = handle.as_ref();
        let range = range.unwrap_or_else(|| {
            // Scan text for @handle
            let search = format!("@{}", handle);
            self.find_substring(&search).unwrap_or(0..0)
        });

        let mut facet_candidates = self.facet_candidates;
        facet_candidates.push(FacetCandidate::Mention { range, did: None });

        RichTextBuilder {
            text: self.text,
            facet_candidates,
            #[cfg(feature = "api_bluesky")]
            embed_candidates: self.embed_candidates,
            _state: PhantomData,
        }
    }
}

impl<S> RichTextBuilder<S> {
    /// Set the text content
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
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
        self.embed_candidates.push(EmbedCandidate::External {
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
