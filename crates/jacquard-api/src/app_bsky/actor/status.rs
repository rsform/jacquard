///A declaration of a Bluesky account status.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub duration_minutes: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub embed: std::option::Option<StatusRecordEmbed<'a>>,
    #[serde(borrow)]
    pub status: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum StatusRecordEmbed<'a> {
    #[serde(rename = "app.bsky.embed.external")]
    External(Box<crate::app_bsky::embed::external::ExternalRecord<'a>>),
}
