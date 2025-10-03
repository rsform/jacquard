#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubmitPlcOperationInput<'a> {
    #[serde(borrow)]
    pub operation: jacquard_common::types::value::Data<'a>,
}
