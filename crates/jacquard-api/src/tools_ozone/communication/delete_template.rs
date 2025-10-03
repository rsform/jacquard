#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTemplateInput<'a> {
    #[serde(borrow)]
    pub id: jacquard_common::CowStr<'a>,
}
