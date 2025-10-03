#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListNotificationsParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub limit: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub priority: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reasons: std::option::Option<Vec<jacquard_common::CowStr<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub seen_at: std::option::Option<jacquard_common::types::string::Datetime>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListNotificationsOutput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub notifications: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub priority: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub seen_at: std::option::Option<jacquard_common::types::string::Datetime>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Notification<'a> {
    #[serde(borrow)]
    pub author: crate::app_bsky::actor::ProfileView<'a>,
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    pub is_read: bool,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<Vec<crate::com_atproto::label::Label<'a>>>,
    #[serde(borrow)]
    pub reason: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reason_subject: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
    #[serde(borrow)]
    pub record: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
