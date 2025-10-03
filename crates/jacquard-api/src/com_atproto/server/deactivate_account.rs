#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeactivateAccountInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub delete_after: std::option::Option<jacquard_common::types::string::Datetime>,
}
