#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Like<'a> {
    #[serde(borrow)]
    pub actor: crate::app_bsky::actor::ProfileView<'a>,
    pub created_at: jacquard_common::types::string::Datetime,
    pub indexed_at: jacquard_common::types::string::Datetime,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLikesParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub limit: std::option::Option<i64>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLikesOutput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub likes: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
