///Deprecated: use facets instead.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entity<'a> {
    pub index: jacquard_common::types::value::Data<'a>,
    pub r#type: jacquard_common::CowStr<'a>,
    pub value: jacquard_common::CowStr<'a>,
}
///Record containing a Bluesky post.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Post<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed: Option<RecordEmbed<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<jacquard_common::types::value::Data<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<test_generated::app_bsky::richtext::Facet<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<RecordLabels<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub langs: Option<Vec<jacquard_common::types::string::Language>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<jacquard_common::CowStr<'a>>>,
    pub text: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum RecordEmbed<'a> {
    #[serde(rename = "app.bsky.embed.images")]
    Images(Box<test_generated::app_bsky::embed::Images<'a>>),
    #[serde(rename = "app.bsky.embed.video")]
    Video(Box<test_generated::app_bsky::embed::Video<'a>>),
    #[serde(rename = "app.bsky.embed.external")]
    External(Box<test_generated::app_bsky::embed::External<'a>>),
    #[serde(rename = "app.bsky.embed.record")]
    Record(Box<test_generated::app_bsky::embed::Record<'a>>),
    #[serde(rename = "app.bsky.embed.recordWithMedia")]
    RecordWithMedia(Box<test_generated::app_bsky::embed::RecordWithMedia<'a>>),
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum RecordLabels<'a> {
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    SelfLabels(Box<test_generated::com_atproto::label::SelfLabels<'a>>),
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRef<'a> {
    pub parent: test_generated::com_atproto::repo::StrongRef<'a>,
    pub root: test_generated::com_atproto::repo::StrongRef<'a>,
}
///Deprecated. Use app.bsky.richtext instead -- A text segment. Start is inclusive, end is exclusive. Indices are for utf16-encoded strings.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextSlice<'a> {
    pub end: i64,
    pub start: i64,
}
