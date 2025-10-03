///Record representing a block relationship against an entire an entire list of accounts (actors).
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Listblock<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::AtUri<'a>,
}
