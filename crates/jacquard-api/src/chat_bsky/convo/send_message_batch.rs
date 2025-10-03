#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub message: crate::chat_bsky::convo::MessageInput<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageBatchInput<'a> {
    #[serde(borrow)]
    pub items: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageBatchOutput<'a> {
    #[serde(borrow)]
    pub items: Vec<crate::chat_bsky::convo::MessageView<'a>>,
}
