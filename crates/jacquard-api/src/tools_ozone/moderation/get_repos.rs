#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetReposParams<'a> {
    #[serde(borrow)]
    pub dids: Vec<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetReposOutput<'a> {
    #[serde(borrow)]
    pub repos: Vec<jacquard_common::types::value::Data<'a>>,
}
