#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateActorAccessInput<'a> {
    #[serde(borrow)]
    pub actor: jacquard_common::types::string::Did<'a>,
    pub allow_access: bool,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub r#ref: std::option::Option<jacquard_common::CowStr<'a>>,
}
