#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetSuggestedFollowsByActorParams<'a> {
    #[serde(borrow)]
    pub actor: jacquard_common::types::ident::AtIdentifier<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetSuggestedFollowsByActorOutput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub is_fallback: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub rec_id: std::option::Option<i64>,
    #[serde(borrow)]
    pub suggestions: Vec<crate::app_bsky::actor::ProfileView<'a>>,
}
