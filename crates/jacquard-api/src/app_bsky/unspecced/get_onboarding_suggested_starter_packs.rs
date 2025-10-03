#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetOnboardingSuggestedStarterPacksParams {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub limit: std::option::Option<i64>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetOnboardingSuggestedStarterPacksOutput<'a> {
    #[serde(borrow)]
    pub starter_packs: Vec<crate::app_bsky::graph::StarterPackView<'a>>,
}
