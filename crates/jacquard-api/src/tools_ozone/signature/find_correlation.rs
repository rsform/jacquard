#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FindCorrelationParams<'a> {
    #[serde(borrow)]
    pub dids: Vec<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FindCorrelationOutput<'a> {
    #[serde(borrow)]
    pub details: Vec<crate::tools_ozone::signature::SigDetail<'a>>,
}
