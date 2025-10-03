#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountInfosParams<'a> {
    #[serde(borrow)]
    pub dids: Vec<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountInfosOutput<'a> {
    #[serde(borrow)]
    pub infos: Vec<crate::com_atproto::admin::AccountView<'a>>,
}
