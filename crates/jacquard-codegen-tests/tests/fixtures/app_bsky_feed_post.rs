// Adapted from jacquard-api for round-trip testing
// This version uses #[derive(LexiconSchema)] to test the derive macro

use jacquard_common::CowStr;
use jacquard_derive::LexiconSchema;
use jacquard_common::types::string::{AtUri, Datetime, Language};

/// Deprecated: use facets instead.
#[derive(LexiconSchema)]
#[lexicon(nsid = "app.bsky.feed.post", fragment = "entity")]
pub struct Entity<'a> {
    pub index: TextSlice<'a>,
    /// Expected values are 'mention' and 'link'.
    pub r#type: CowStr<'a>,
    pub value: CowStr<'a>,
}

#[derive(LexiconSchema)]
#[lexicon(nsid = "app.bsky.feed.post", record, key = "tid")]
pub struct Post<'a> {
    /// Client-declared timestamp when this post was originally created.
    pub created_at: Datetime,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed: Option<PostEmbed<'a>>,

    /// DEPRECATED: replaced by app.bsky.richtext.facet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<Entity<'a>>>,

    /// Annotations of text (mentions, URLs, hashtags, etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<Facet<'a>>>,

    /// Self-label values for this post. Effectively content warnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<SelfLabels<'a>>,

    /// Indicates human language of post primary text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[lexicon(max_length = 3)]
    pub langs: Option<Vec<Language>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyRef<'a>>,

    /// Additional hashtags, in addition to any included in post text and facets.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[lexicon(max_length = 8)]
    pub tags: Option<Vec<CowStr<'a>>>,

    /// The primary post content. May be an empty string, if there are embeds.
    #[lexicon(max_length = 3000, max_graphemes = 300)]
    pub text: CowStr<'a>,
}

#[derive(LexiconSchema)]
#[lexicon(nsid = "app.bsky.feed.post", fragment = "replyRef")]
pub struct ReplyRef<'a> {
    #[lexicon(ref = "com.atproto.repo.strongRef")]
    pub parent: StrongRef<'a>,

    #[lexicon(ref = "com.atproto.repo.strongRef")]
    pub root: StrongRef<'a>,
}

/// Deprecated. Use app.bsky.richtext instead -- A text segment. Start is inclusive, end is exclusive. Indices are for utf16-encoded strings.
#[derive(LexiconSchema)]
#[lexicon(nsid = "app.bsky.feed.post", fragment = "textSlice")]
pub struct TextSlice<'a> {
    #[lexicon(minimum = 0)]
    pub end: i64,

    #[lexicon(minimum = 0)]
    pub start: i64,

    #[serde(skip)]
    _phantom: std::marker::PhantomData<&'a ()>,
}

// Placeholder types that would come from other lexicons
pub struct PostEmbed<'a>(std::marker::PhantomData<&'a ()>);
pub struct Facet<'a>(std::marker::PhantomData<&'a ()>);
pub struct SelfLabels<'a>(std::marker::PhantomData<&'a ()>);
pub struct StrongRef<'a>(std::marker::PhantomData<&'a ()>);
