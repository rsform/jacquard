///A declaration of a Bluesky account profile.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub avatar: std::option::Option<jacquard_common::types::blob::Blob<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub banner: std::option::Option<jacquard_common::types::blob::Blob<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub created_at: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub display_name: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub joined_via_starter_pack: std::option::Option<
        crate::com_atproto::repo::strong_ref::StrongRef<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<ProfileRecordLabels<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub pinned_post: std::option::Option<
        crate::com_atproto::repo::strong_ref::StrongRef<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub pronouns: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub website: std::option::Option<jacquard_common::types::string::Uri<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ProfileRecordLabels<'a> {
    #[serde(rename = "com.atproto.label.defs#selfLabels")]
    DefsSelfLabels(Box<crate::com_atproto::label::SelfLabels<'a>>),
}
