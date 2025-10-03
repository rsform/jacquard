#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetUnreadCountParams {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub priority: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub seen_at: std::option::Option<jacquard_common::types::string::Datetime>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetUnreadCountOutput<'a> {
    pub count: i64,
}
