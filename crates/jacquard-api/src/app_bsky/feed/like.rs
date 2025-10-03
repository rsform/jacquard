///Record declaring a 'like' of a piece of subject content.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Like<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub subject: crate::com_atproto::repo::strong_ref::StrongRef<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub via: std::option::Option<crate::com_atproto::repo::strong_ref::StrongRef<'a>>,
}
