#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MuteActorListInput<'a> {
    #[serde(borrow)]
    pub list: jacquard_common::types::string::AtUri<'a>,
}
