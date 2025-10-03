#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSeenInput<'a> {
    pub seen_at: jacquard_common::types::string::Datetime,
}
