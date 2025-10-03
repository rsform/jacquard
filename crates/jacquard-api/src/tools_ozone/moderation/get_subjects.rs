#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetSubjectsParams<'a> {
    #[serde(borrow)]
    pub subjects: Vec<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetSubjectsOutput<'a> {
    #[serde(borrow)]
    pub subjects: Vec<crate::tools_ozone::moderation::SubjectView<'a>>,
}
