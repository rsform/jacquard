#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetStarterPackParams<'a> {
    #[serde(borrow)]
    pub starter_pack: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetStarterPackOutput<'a> {
    #[serde(borrow)]
    pub starter_pack: crate::app_bsky::graph::StarterPackView<'a>,
}
