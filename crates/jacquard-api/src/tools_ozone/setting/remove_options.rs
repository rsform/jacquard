#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoveOptionsInput<'a> {
    #[serde(borrow)]
    pub keys: Vec<jacquard_common::types::string::Nsid<'a>>,
    #[serde(borrow)]
    pub scope: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoveOptionsOutput<'a> {}
