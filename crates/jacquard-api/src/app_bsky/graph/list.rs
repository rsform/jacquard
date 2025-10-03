///Record representing a list of accounts (actors). Scope includes both moderation-oriented lists and curration-oriented lists.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct List<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub avatar: std::option::Option<jacquard_common::types::blob::Blob<'a>>,
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description_facets: std::option::Option<
        Vec<crate::app_bsky::richtext::facet::Facet<'a>>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<ListRecordLabels<'a>>,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub purpose: crate::app_bsky::graph::ListPurpose<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ListRecordLabels<'a> {
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    DefsSelfLabels(Box<crate::com_atproto::label::SelfLabels<'a>>),
}
