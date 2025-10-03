#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAgeAssuranceStateOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::app_bsky::unspecced::AgeAssuranceState<'a>,
}
