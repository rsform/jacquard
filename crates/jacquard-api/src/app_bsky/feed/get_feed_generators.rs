#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetFeedGeneratorsParams<'a> {
    #[serde(borrow)]
    pub feeds: Vec<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetFeedGeneratorsOutput<'a> {
    #[serde(borrow)]
    pub feeds: Vec<crate::app_bsky::feed::GeneratorView<'a>>,
}
