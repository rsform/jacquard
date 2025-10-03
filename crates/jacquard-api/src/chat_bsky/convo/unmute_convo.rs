#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnmuteConvoInput<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnmuteConvoOutput<'a> {
    #[serde(borrow)]
    pub convo: crate::chat_bsky::convo::ConvoView<'a>,
}
