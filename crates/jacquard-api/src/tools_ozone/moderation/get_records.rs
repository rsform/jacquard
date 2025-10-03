#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordsParams<'a> {
    #[serde(borrow)]
    pub uris: Vec<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordsOutput<'a> {
    #[serde(borrow)]
    pub records: Vec<jacquard_common::types::value::Data<'a>>,
}
