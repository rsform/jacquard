#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotifyOfUpdateInput<'a> {
    #[serde(borrow)]
    pub hostname: jacquard_common::CowStr<'a>,
}
