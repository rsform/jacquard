#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageForSelfInput<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub message_id: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageForSelfOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::chat_bsky::convo::DeletedMessageView<'a>,
}
