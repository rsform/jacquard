#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageContextParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub after: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub before: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub convo_id: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub message_id: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageContextOutput<'a> {
    #[serde(borrow)]
    pub messages: Vec<jacquard_common::types::value::Data<'a>>,
}
