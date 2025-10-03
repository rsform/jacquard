///Error object for failed verifications.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrantError<'a> {
    #[serde(borrow)]
    pub error: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrantVerificationsInput<'a> {
    #[serde(borrow)]
    pub verifications: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrantVerificationsOutput<'a> {
    #[serde(borrow)]
    pub failed_verifications: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub verifications: Vec<crate::tools_ozone::verification::VerificationView<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub created_at: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
