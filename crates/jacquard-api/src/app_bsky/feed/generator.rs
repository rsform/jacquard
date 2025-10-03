///Record declaring of the existence of a feed generator, and containing metadata about it. The record can exist in any repository.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Generator<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub accepts_interactions: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub avatar: std::option::Option<jacquard_common::types::blob::Blob<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub content_mode: std::option::Option<jacquard_common::CowStr<'a>>,
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description_facets: std::option::Option<
        Vec<crate::app_bsky::richtext::facet::Facet<'a>>,
    >,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<GeneratorRecordLabels<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum GeneratorRecordLabels<'a> {
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    DefsSelfLabels(Box<crate::com_atproto::label::SelfLabels<'a>>),
}
