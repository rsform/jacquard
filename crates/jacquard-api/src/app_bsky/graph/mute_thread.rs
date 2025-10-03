#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MuteThreadInput<'a> {
    #[serde(borrow)]
    pub root: jacquard_common::types::string::AtUri<'a>,
}
