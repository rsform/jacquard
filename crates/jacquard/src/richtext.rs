//! Rich text utilities for Bluesky posts
//!
//! Provides parsing and building of rich text with facets (mentions, links, tags)
//! and detection of embed candidates (record and external embeds).

use crate::common::CowStr;
use std::marker::PhantomData;
use std::ops::Range;

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
pub struct RichTextBuilder<'a, State> {
    text: String,
    facet_candidates: Vec<FacetCandidate<'a>>,
    _state: PhantomData<State>,
}

/// Internal representation of facet before resolution
#[derive(Debug, Clone)]
enum FacetCandidate<'a> {
    Mention {
        handle_or_did: CowStr<'a>,
        range: Range<usize>,
        /// DID when provided, otherwise resolved later
        did: Option<Did<'static>>,
    },
    Link {
        url: CowStr<'a>,
        range: Range<usize>,
    },
    Tag {
        tag: CowStr<'a>,
        range: Range<usize>,
    },
}

impl<'a> RichTextBuilder<'a, Unresolved> {
    /// Entry point for parsing text with automatic facet detection
    pub fn parse(text: impl Into<String>) -> Self {
        todo!("Task 2")
    }
}

impl<'a> RichTextBuilder<'a, Resolved> {
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
