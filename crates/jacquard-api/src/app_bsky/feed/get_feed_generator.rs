#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetFeedGeneratorParams<'a> {
    #[serde(borrow)]
    pub feed: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetFeedGeneratorOutput<'a> {
    pub is_online: bool,
    pub is_valid: bool,
    #[serde(borrow)]
    pub view: crate::app_bsky::feed::GeneratorView<'a>,
}
