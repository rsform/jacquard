///Allow replies from actors who follow you.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FollowerRule<'a> {}
///Allow replies from actors you follow.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FollowingRule<'a> {}
///Allow replies from actors on a list.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListRule<'a> {
    #[serde(borrow)]
    pub list: jacquard_common::types::string::AtUri<'a>,
}
///Record defining interaction gating rules for a thread (aka, reply controls). The record key (rkey) of the threadgate record must match the record key of the thread's root post, and that record must be in the same repository.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Threadgate<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub allow: std::option::Option<Vec<jacquard_common::types::value::Data<'a>>>,
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub hidden_replies: std::option::Option<
        Vec<jacquard_common::types::string::AtUri<'a>>,
    >,
    #[serde(borrow)]
    pub post: jacquard_common::types::string::AtUri<'a>,
}
///Allow replies from actors mentioned in your post.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MentionRule<'a> {}
