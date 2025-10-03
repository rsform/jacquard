#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPostsParams<'a> {
    #[serde(borrow)]
    pub uris: Vec<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPostsOutput<'a> {
    #[serde(borrow)]
    pub posts: Vec<crate::app_bsky::feed::PostView<'a>>,
}
