#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelerPolicies<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub label_value_definitions: std::option::Option<
        Vec<crate::com_atproto::label::LabelValueDefinition<'a>>,
    >,
    #[serde(borrow)]
    pub label_values: Vec<crate::com_atproto::label::LabelValue<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelerView<'a> {
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(borrow)]
    pub creator: crate::app_bsky::actor::ProfileView<'a>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<Vec<crate::com_atproto::label::Label<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub like_count: std::option::Option<i64>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelerViewDetailed<'a> {
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(borrow)]
    pub creator: crate::app_bsky::actor::ProfileView<'a>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<Vec<crate::com_atproto::label::Label<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub like_count: std::option::Option<i64>,
    #[serde(borrow)]
    pub policies: crate::app_bsky::labeler::LabelerPolicies<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reason_types: std::option::Option<
        Vec<crate::com_atproto::moderation::ReasonType<'a>>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subject_collections: std::option::Option<
        Vec<jacquard_common::types::string::Nsid<'a>>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subject_types: std::option::Option<
        Vec<crate::com_atproto::moderation::SubjectType<'a>>,
    >,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelerViewerState<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub like: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
}
pub mod get_services;
pub mod service;
