#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetConvoForMembersParams<'a> {
    #[serde(borrow)]
    pub members: Vec<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetConvoForMembersOutput<'a> {
    #[serde(borrow)]
    pub convo: crate::chat_bsky::convo::ConvoView<'a>,
}
