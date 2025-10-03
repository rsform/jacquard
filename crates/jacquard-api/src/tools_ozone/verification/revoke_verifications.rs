#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RevokeVerificationsInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub revoke_reason: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub uris: Vec<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RevokeVerificationsOutput<'a> {
    #[serde(borrow)]
    pub failed_revocations: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub revoked_verifications: Vec<jacquard_common::types::string::AtUri<'a>>,
}
///Error object for failed revocations
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RevokeError<'a> {
    #[serde(borrow)]
    pub error: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
