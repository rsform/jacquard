#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertOptionInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub key: jacquard_common::types::string::Nsid<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub manager_role: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub scope: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub value: jacquard_common::types::value::Data<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertOptionOutput<'a> {
    #[serde(borrow)]
    pub option: crate::tools_ozone::setting::Option<'a>,
}
