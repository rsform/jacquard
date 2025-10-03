#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetStarterPacksParams<'a> {
    #[serde(borrow)]
    pub uris: Vec<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetStarterPacksOutput<'a> {
    #[serde(borrow)]
    pub starter_packs: Vec<crate::app_bsky::graph::StarterPackViewBasic<'a>>,
}
