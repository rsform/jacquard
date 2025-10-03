#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountCodes<'a> {
    #[serde(borrow)]
    pub account: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub codes: Vec<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteCodesInput<'a> {
    pub code_count: i64,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub for_accounts: std::option::Option<Vec<jacquard_common::types::string::Did<'a>>>,
    pub use_count: i64,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteCodesOutput<'a> {
    #[serde(borrow)]
    pub codes: Vec<jacquard_common::types::value::Data<'a>>,
}
