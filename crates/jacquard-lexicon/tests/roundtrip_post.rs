// Round-trip test: JSON → LexiconDoc and Rust+derive → LexiconDoc should match
//
// This test verifies that the derive macro generates lexicon documents that match
// the original JSON schemas.

use jacquard_common::CowStr;
use jacquard_common::types::string::{Datetime, Language};
use jacquard_lexicon::lexicon::LexiconDoc;
use serde::{Deserialize, Serialize};

/// Deprecated. Use app.bsky.richtext instead -- A text segment. Start is inclusive, end is exclusive. Indices are for utf16-encoded strings.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    jacquard_derive::IntoStatic,
    jacquard_derive::LexiconSchema,
)]
#[lexicon(nsid = "app.bsky.feed.post", fragment = "textSlice")]
#[serde(rename_all = "camelCase")]
pub struct TextSlice {
    #[lexicon(minimum = 0)]
    pub end: i64,

    #[lexicon(minimum = 0)]
    pub start: i64,
}

/// Deprecated: use facets instead.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    jacquard_derive::IntoStatic,
    jacquard_derive::LexiconSchema,
)]
#[lexicon(nsid = "app.bsky.feed.post", fragment = "entity")]
#[serde(rename_all = "camelCase")]
pub struct Entity<'a> {
    #[lexicon(ref = "#textSlice")]
    pub index: TextSlice,
    #[serde(borrow)]
    /// Expected values are 'mention' and 'link'.
    pub r#type: CowStr<'a>,
    #[serde(borrow)]
    pub value: CowStr<'a>,
}

// the inner parameters we can shorthand bc they're in other namespaces
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    jacquard_derive::IntoStatic,
    jacquard_derive::LexiconSchema,
)]
#[lexicon(nsid = "app.bsky.feed.post", fragment = "replyRef")]
#[serde(rename_all = "camelCase")]
pub struct ReplyRef<'a> {
    #[serde(borrow)]
    #[lexicon(ref = "com.atproto.repo.strongRef")]
    pub parent: CowStr<'a>,

    #[serde(borrow)]
    #[lexicon(ref = "com.atproto.repo.strongRef")]
    pub root: CowStr<'a>,
}

/// Union for embed field - these we can shorthand bc they're in other namespaces
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
#[jacquard_derive::lexicon_union]
#[serde(tag = "$type")]
pub enum PostEmbed<'a> {
    #[serde(borrow)]
    #[serde(rename = "app.bsky.embed.images")]
    Images(CowStr<'a>),
    #[serde(rename = "app.bsky.embed.video")]
    Video(CowStr<'a>),
    #[serde(rename = "app.bsky.embed.external")]
    External(CowStr<'a>),
    #[serde(rename = "app.bsky.embed.record")]
    Record(CowStr<'a>),
    #[serde(rename = "app.bsky.embed.recordWithMedia")]
    RecordWithMedia(CowStr<'a>),
}

/// Union for labels field
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
#[jacquard_derive::lexicon_union]
#[serde(tag = "$type")]
pub enum PostLabels<'a> {
    #[serde(borrow)]
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    SelfLabels(CowStr<'a>),
}

/// Record containing a Bluesky post.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    jacquard_derive::IntoStatic,
    jacquard_derive::LexiconSchema,
)]
#[lexicon(nsid = "app.bsky.feed.post", record, key = "tid")]
#[serde(rename_all = "camelCase")]
pub struct Post<'a> {
    /// Client-declared timestamp when this post was originally created.
    pub created_at: Datetime,

    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[lexicon(union)]
    pub embed: Option<PostEmbed<'a>>,

    /// DEPRECATED: replaced by app.bsky.richtext.facet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub entities: Option<Vec<Entity<'a>>>,

    /// Annotations of text (mentions, URLs, hashtags, etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    #[lexicon(ref = "app.bsky.richtext.facet")]
    pub facets: Option<Vec<CowStr<'a>>>,

    /// Self-label values for this post. Effectively content warnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    #[lexicon(union)]
    pub labels: Option<PostLabels<'a>>,

    /// Indicates human language of post primary text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[lexicon(max_items = 3)]
    pub langs: Option<Vec<Language>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub reply: Option<ReplyRef<'a>>,

    /// Additional hashtags, in addition to any included in post text and facets.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    #[lexicon(max_items = 8, item_max_length = 640, item_max_graphemes = 64)]
    pub tags: Option<Vec<CowStr<'a>>>,

    /// The primary post content. May be an empty string, if there are embeds.
    #[serde(borrow)]
    #[lexicon(max_length = 3000, max_graphemes = 300)]
    pub text: CowStr<'a>,
}

#[test]
fn test_union_refs_exist() {
    // Test that the lexicon_union macro generated the constants
    assert_eq!(PostEmbed::LEXICON_UNION_REFS.len(), 5);
    assert_eq!(PostLabels::LEXICON_UNION_REFS.len(), 1);
}

#[test]
fn test_post_roundtrip() {
    // Load the JSON lexicon
    let json_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_lexicons/post.json"
    );
    let json_content = std::fs::read_to_string(json_path).expect("failed to read post.json");

    let json_doc: LexiconDoc =
        serde_json::from_str(&json_content).expect("failed to parse post.json");

    // Get the complete lexicon doc from the registry (merges all defs)
    let registry = jacquard_lexicon::schema::global_registry();
    let derived_doc = registry
        .get("app.bsky.feed.post")
        .expect("schema not found in registry");

    let doc_str =
        serde_json::to_string_pretty(&derived_doc).expect("failed to serialize derived doc");
    println!("{}", doc_str);

    // Compare NSIDs
    assert_eq!(
        derived_doc.id.as_ref(),
        json_doc.id.as_ref(),
        "NSID mismatch"
    );
    assert_eq!(derived_doc.id.as_ref(), "app.bsky.feed.post");

    // Compare defs - both should have: main, entity, replyRef, textSlice
    assert_eq!(
        derived_doc.defs.len(),
        json_doc.defs.len(),
        "Number of defs mismatch"
    );

    for (def_name, json_def) in &json_doc.defs {
        let derived_def = derived_doc
            .defs
            .get(def_name.as_str())
            .unwrap_or_else(|| panic!("derived doc missing def: {}", def_name));

        // Normalize both defs (sort required fields) for comparison
        let mut normalized_derived = derived_def.clone();
        let mut normalized_json = json_def.clone();
        normalize_user_type(&mut normalized_derived);
        normalize_user_type(&mut normalized_json);

        // Compare the structure (this will show where they differ)
        assert_eq!(
            normalized_derived, normalized_json,
            "Def '{}' doesn't match:\nDerived: {:#?}\nJSON: {:#?}",
            def_name, normalized_derived, normalized_json
        );
    }

    // If we got here, they match!
    println!("✓ Round-trip test passed: Rust → lexicon matches JSON schema");
}

/// Normalize a user type for comparison by sorting required fields
fn normalize_user_type(user_type: &mut jacquard_lexicon::lexicon::LexUserType) {
    use jacquard_lexicon::lexicon::*;

    match user_type {
        LexUserType::Record(rec) => match &mut rec.record {
            LexRecordRecord::Object(obj) => {
                normalize_object(obj);
            }
        },
        LexUserType::Object(obj) => {
            normalize_object(obj);
        }
        LexUserType::XrpcQuery(query) => {
            if let Some(LexXrpcQueryParameter::Params(params)) = &mut query.parameters {
                if let Some(ref mut req) = params.required {
                    req.sort();
                }
            }
        }
        LexUserType::XrpcProcedure(proc) => {
            if let Some(input) = &mut proc.input {
                if let Some(LexXrpcBodySchema::Object(obj)) = &mut input.schema {
                    normalize_object(obj);
                }
            }
        }
        _ => {}
    }
}

fn normalize_object(obj: &mut jacquard_lexicon::lexicon::LexObject) {
    // Sort required fields
    if let Some(ref mut req) = obj.required {
        req.sort();
    }

    // Recursively normalize nested objects
    for prop in obj.properties.values_mut() {
        normalize_property(prop);
    }
}

fn normalize_property(prop: &mut jacquard_lexicon::lexicon::LexObjectProperty) {
    use jacquard_lexicon::lexicon::*;

    match prop {
        LexObjectProperty::Object(obj) => normalize_object(obj),
        LexObjectProperty::Array(arr) => {
            if let LexArrayItem::Object(obj) = &mut arr.items {
                normalize_object(obj);
            }
        }
        _ => {}
    }
}
