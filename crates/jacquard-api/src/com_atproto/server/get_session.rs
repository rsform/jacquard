#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionOutput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub active: std::option::Option<bool>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub did_doc: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub email: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub email_auth_factor: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub email_confirmed: std::option::Option<bool>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub status: std::option::Option<jacquard_common::CowStr<'a>>,
}
