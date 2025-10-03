#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Contact<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub email: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Links<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub privacy_policy: std::option::Option<jacquard_common::types::string::Uri<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub terms_of_service: std::option::Option<jacquard_common::types::string::Uri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DescribeServerOutput<'a> {
    #[serde(borrow)]
    pub available_user_domains: Vec<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub contact: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub invite_code_required: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub links: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub phone_verification_required: std::option::Option<bool>,
}
