#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetServicesParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub detailed: std::option::Option<bool>,
    #[serde(borrow)]
    pub dids: Vec<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetServicesOutput<'a> {
    #[serde(borrow)]
    pub views: Vec<jacquard_common::types::value::Data<'a>>,
}
