///Deprecated: use facets instead.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entity<'a> {
    #[serde(borrow)]
    pub index: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub r#type: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub value: jacquard_common::CowStr<'a>,
}
///Record containing a Bluesky post.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Post<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub embed: std::option::Option<PostRecordEmbed<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub entities: std::option::Option<Vec<jacquard_common::types::value::Data<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub facets: std::option::Option<Vec<crate::app_bsky::richtext::facet::Facet<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<PostRecordLabels<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub langs: std::option::Option<Vec<jacquard_common::types::string::Language>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reply: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub tags: std::option::Option<Vec<jacquard_common::CowStr<'a>>>,
    #[serde(borrow)]
    pub text: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum PostRecordEmbed<'a> {
    #[serde(rename = "app.bsky.embed.images")]
    Images(Box<crate::app_bsky::embed::images::Images<'a>>),
    #[serde(rename = "app.bsky.embed.video")]
    Video(Box<crate::app_bsky::embed::video::Video<'a>>),
    #[serde(rename = "app.bsky.embed.external")]
    External(Box<crate::app_bsky::embed::external::ExternalRecord<'a>>),
    #[serde(rename = "app.bsky.embed.record")]
    Record(Box<crate::app_bsky::embed::record::Record<'a>>),
    #[serde(rename = "app.bsky.embed.recordWithMedia")]
    RecordWithMedia(Box<crate::app_bsky::embed::record_with_media::RecordWithMedia<'a>>),
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum PostRecordLabels<'a> {
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    DefsSelfLabels(Box<crate::com_atproto::label::SelfLabels<'a>>),
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRef<'a> {
    #[serde(borrow)]
    pub parent: crate::com_atproto::repo::strong_ref::StrongRef<'a>,
    #[serde(borrow)]
    pub root: crate::com_atproto::repo::strong_ref::StrongRef<'a>,
}
///Deprecated. Use app.bsky.richtext instead -- A text segment. Start is inclusive, end is exclusive. Indices are for utf16-encoded strings.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextSlice<'a> {
    pub end: i64,
    pub start: i64,
}
