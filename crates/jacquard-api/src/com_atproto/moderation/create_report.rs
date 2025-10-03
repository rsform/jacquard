#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateReportInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub mod_tool: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reason: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub reason_type: crate::com_atproto::moderation::ReasonType<'a>,
    #[serde(borrow)]
    pub subject: CreateReportInputRecordSubject<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum CreateReportInputRecordSubject<'a> {
    #[serde(rename = "com.atproto.admin.defs#repoRef")]
    DefsRepoRef(Box<crate::com_atproto::admin::RepoRef<'a>>),
    #[serde(rename = "com.atproto.repo.strongRef")]
    StrongRef(Box<crate::com_atproto::repo::strong_ref::StrongRef<'a>>),
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateReportOutput<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    pub id: i64,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reason: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub reason_type: crate::com_atproto::moderation::ReasonType<'a>,
    #[serde(borrow)]
    pub reported_by: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub subject: CreateReportOutputRecordSubject<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum CreateReportOutputRecordSubject<'a> {
    #[serde(rename = "com.atproto.admin.defs#repoRef")]
    DefsRepoRef(Box<crate::com_atproto::admin::RepoRef<'a>>),
    #[serde(rename = "com.atproto.repo.strongRef")]
    StrongRef(Box<crate::com_atproto::repo::strong_ref::StrongRef<'a>>),
}
///Moderation tool information for tracing the source of the action
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModTool<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub meta: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
}
