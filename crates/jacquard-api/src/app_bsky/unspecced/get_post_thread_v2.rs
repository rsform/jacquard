#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPostThreadV2Params<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub above: std::option::Option<bool>,
    #[serde(borrow)]
    pub anchor: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub below: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub branching_factor: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub prioritize_followed_users: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub sort: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPostThreadV2Output<'a> {
    pub has_other_replies: bool,
    #[serde(borrow)]
    pub thread: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub threadgate: std::option::Option<crate::app_bsky::feed::ThreadgateView<'a>>,
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
    #[serde(rename = "app.bsky.unspecced.defs#threadItemNoUnauthenticated")]
    DefsThreadItemNoUnauthenticated(
        Box<crate::app_bsky::unspecced::ThreadItemNoUnauthenticated<'a>>,
    ),
    #[serde(rename = "app.bsky.unspecced.defs#threadItemNotFound")]
    DefsThreadItemNotFound(Box<crate::app_bsky::unspecced::ThreadItemNotFound<'a>>),
    #[serde(rename = "app.bsky.unspecced.defs#threadItemBlocked")]
    DefsThreadItemBlocked(Box<crate::app_bsky::unspecced::ThreadItemBlocked<'a>>),
}
