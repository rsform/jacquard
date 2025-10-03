#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetEventParams {
    pub id: i64,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetEventOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::tools_ozone::moderation::ModEventViewDetail<'a>,
}
