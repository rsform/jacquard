#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountHandleInput<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
}
