///A declaration of the existence of labeler service.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Service<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<ServiceRecordLabels<'a>>,
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
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ServiceRecordLabels<'a> {
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    DefsSelfLabels(Box<crate::com_atproto::label::SelfLabels<'a>>),
}
