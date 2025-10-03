///Disables embedding of this post.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DisableRule<'a> {}
///Record defining interaction rules for a post. The record key (rkey) of the postgate record must match the record key of the post, and that record must be in the same repository.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Postgate<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub detached_embedding_uris: std::option::Option<
        Vec<jacquard_common::types::string::AtUri<'a>>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub embedding_rules: std::option::Option<
        Vec<jacquard_common::types::value::Data<'a>>,
    >,
    #[serde(borrow)]
    pub post: jacquard_common::types::string::AtUri<'a>,
}
