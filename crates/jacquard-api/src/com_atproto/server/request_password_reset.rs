#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestPasswordResetInput<'a> {
    #[serde(borrow)]
    pub email: jacquard_common::CowStr<'a>,
}
