#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPostThreadOtherV2Params<'a> {
    #[serde(borrow)]
    pub anchor: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub prioritize_followed_users: std::option::Option<bool>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPostThreadOtherV2Output<'a> {
    #[serde(borrow)]
    pub thread: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadItem<'a> {
    pub depth: i64,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
    #[serde(borrow)]
    pub value: ThreadItemRecordValue<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ThreadItemRecordValue<'a> {
    #[serde(rename = "app.bsky.unspecced.defs#threadItemPost")]
    DefsThreadItemPost(Box<crate::app_bsky::unspecced::ThreadItemPost<'a>>),
}
