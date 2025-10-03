#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteCodeInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub for_account: std::option::Option<jacquard_common::types::string::Did<'a>>,
    pub use_count: i64,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteCodeOutput<'a> {
    #[serde(borrow)]
    pub code: jacquard_common::CowStr<'a>,
}
