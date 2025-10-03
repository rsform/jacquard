#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPushInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub age_restricted: std::option::Option<bool>,
    #[serde(borrow)]
    pub app_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub platform: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub service_did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub token: jacquard_common::CowStr<'a>,
}
