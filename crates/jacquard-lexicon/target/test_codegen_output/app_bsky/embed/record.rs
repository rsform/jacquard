#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Record<'a> {
    pub record: test_generated::com_atproto::repo::StrongRef<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    pub record: RecordRecord<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewBlocked<'a> {
    pub author: jacquard_common::types::value::Data<'a>,
    pub blocked: bool,
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewDetached<'a> {
    pub detached: bool,
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewNotFound<'a> {
    pub not_found: bool,
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewRecord<'a> {
    pub author: jacquard_common::types::value::Data<'a>,
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<jacquard_common::types::value::Data<'a>>>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<test_generated::com_atproto::label::Label<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub like_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repost_count: Option<i64>,
    pub uri: jacquard_common::types::string::AtUri<'a>,
    pub value: jacquard_common::types::value::Data<'a>,
}
