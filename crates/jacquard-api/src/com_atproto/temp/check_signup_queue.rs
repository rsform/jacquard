#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckSignupQueueOutput<'a> {
    pub activated: bool,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub estimated_time_ms: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub place_in_queue: std::option::Option<i64>,
}
