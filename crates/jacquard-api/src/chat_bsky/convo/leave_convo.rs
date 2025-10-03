#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LeaveConvoInput<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LeaveConvoOutput<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub rev: jacquard_common::CowStr<'a>,
}
