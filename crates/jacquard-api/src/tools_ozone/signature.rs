#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SigDetail<'a> {
    #[serde(borrow)]
    pub property: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub value: jacquard_common::CowStr<'a>,
}
pub mod find_correlation;
pub mod find_related_accounts;
pub mod search_accounts;
