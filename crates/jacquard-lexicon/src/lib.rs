pub mod fs;
pub mod lexicon;
pub mod output;
pub mod schema;

// #[lexicon]
// #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
// #[serde(rename_all = "camelCase")]
// pub struct Post<'s> {
//     ///Client-declared timestamp when this post was originally created.
//     pub created_at: jacquard_common::types::string::Datetime,
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub embed: core::option::Option<RecordEmbed<'s>>,
//     ///DEPRECATED: replaced by app.bsky.richtext.facet.
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub entities: core::option::Option<Vec<Entity<'s>>>,
//     ///Annotations of text (mentions, URLs, hashtags, etc)
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub facets: core::option::Option<Vec<jacquard_api::app_bsky::richtext::Facet<'s>>>,
//     ///Self-label values for this post. Effectively content warnings.
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub labels: core::option::Option<RecordLabels<'s>>,
//     ///Indicates human language of post primary text content.
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub langs: core::option::Option<Vec<jacquard_common::types::string::Language>>,
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub reply: core::option::Option<ReplyRef<'s>>,
//     ///Additional hashtags, in addition to any included in post text and facets.
//     #[serde(skip_serializing_if = "core::option::Option::is_none")]
//     pub tags: core::option::Option<Vec<jacquard_common::CowStr<'s>>>,
//     ///The primary post content. May be an empty string, if there are embeds.
//     #[serde(borrow)]
//     pub text: jacquard_common::CowStr<'s>,
// }

// #[open_union]
// #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
// #[serde(tag = "$type")]
// pub enum RecordEmbed<'s> {
//     #[serde(borrow)]
//     #[serde(rename = "app.bsky.embed.images")]
//     EmbedImages(Box<jacquard_api::app_bsky::embed::Images<'s>>),
//     #[serde(rename = "app.bsky.embed.video")]
//     EmbedVideo(Box<jacquard_api::app_bsky::embed::Video<'s>>),
//     #[serde(rename = "app.bsky.embed.external")]
//     EmbedExternal(Box<jacquard_api::app_bsky::embed::External<'s>>),
//     #[serde(rename = "app.bsky.embed.record")]
//     EmbedRecord(Box<jacquard_api::app_bsky::embed::Record<'s>>),
//     #[serde(rename = "app.bsky.embed.recordWithMedia")]
//     EmbedRecordWithMedia(Box<jacquard_api::app_bsky::embed::RecordWithMedia<'s>>),
// }
