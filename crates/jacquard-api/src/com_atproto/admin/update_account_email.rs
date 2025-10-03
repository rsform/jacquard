#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountEmailInput<'a> {
    #[serde(borrow)]
    pub account: jacquard_common::types::ident::AtIdentifier<'a>,
    #[serde(borrow)]
    pub email: jacquard_common::CowStr<'a>,
}
