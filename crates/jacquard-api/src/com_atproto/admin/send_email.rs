#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendEmailInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub comment: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub content: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub recipient_did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub sender_did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subject: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendEmailOutput<'a> {
    pub sent: bool,
}
