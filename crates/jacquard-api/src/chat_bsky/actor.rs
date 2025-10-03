#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileViewBasic<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub associated: std::option::Option<crate::app_bsky::actor::ProfileAssociated<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub avatar: std::option::Option<jacquard_common::types::string::Uri<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub chat_disabled: std::option::Option<bool>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub display_name: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<Vec<crate::com_atproto::label::Label<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub verification: std::option::Option<crate::app_bsky::actor::VerificationState<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<crate::app_bsky::actor::ViewerState<'a>>,
}
pub mod declaration;
pub mod delete_account;
pub mod export_account_data;
