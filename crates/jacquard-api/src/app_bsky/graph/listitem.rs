///Record representing an account's inclusion on a specific list. The AppView will ignore duplicate listitem records.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Listitem<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub list: jacquard_common::types::string::AtUri<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
