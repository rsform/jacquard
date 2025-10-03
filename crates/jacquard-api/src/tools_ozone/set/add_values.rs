#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddValuesInput<'a> {
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub values: Vec<jacquard_common::CowStr<'a>>,
}
