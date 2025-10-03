#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub message: crate::chat_bsky::convo::MessageInput<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::chat_bsky::convo::MessageView<'a>,
}
