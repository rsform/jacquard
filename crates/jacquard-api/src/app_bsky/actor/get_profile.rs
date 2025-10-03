#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetProfileParams<'a> {
    #[serde(borrow)]
    pub actor: jacquard_common::types::ident::AtIdentifier<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetProfileOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::app_bsky::actor::ProfileViewDetailed<'a>,
}
