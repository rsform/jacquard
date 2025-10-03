#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetOnboardingSuggestedStarterPacksSkeletonParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub limit: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetOnboardingSuggestedStarterPacksSkeletonOutput<'a> {
    #[serde(borrow)]
    pub starter_packs: Vec<jacquard_common::types::string::AtUri<'a>>,
}
