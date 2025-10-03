#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetActorMetadataParams<'a> {
    #[serde(borrow)]
    pub actor: jacquard_common::types::string::Did<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetActorMetadataOutput<'a> {
    #[serde(borrow)]
    pub all: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub day: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub month: jacquard_common::types::value::Data<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Metadata<'a> {
    pub convos: i64,
    pub convos_started: i64,
    pub messages_received: i64,
    pub messages_sent: i64,
}
