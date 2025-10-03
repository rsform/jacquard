///Record declaring a social 'follow' relationship of another account. Duplicate follows will be ignored by the AppView.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Follow<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
