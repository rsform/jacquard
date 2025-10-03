#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAllReadInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub status: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAllReadOutput<'a> {
    pub updated_count: i64,
}
