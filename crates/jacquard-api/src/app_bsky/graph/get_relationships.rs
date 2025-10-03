#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRelationshipsParams<'a> {
    #[serde(borrow)]
    pub actor: jacquard_common::types::ident::AtIdentifier<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub others: std::option::Option<
        Vec<jacquard_common::types::ident::AtIdentifier<'a>>,
    >,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRelationshipsOutput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub actor: std::option::Option<jacquard_common::types::string::Did<'a>>,
    #[serde(borrow)]
    pub relationships: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::open_union]
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    thiserror::Error,
    miette::Diagnostic
)]
#[serde(tag = "error", content = "message")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum GetRelationshipsError<'a> {
    ///the primary actor at-identifier could not be resolved
    #[serde(rename = "ActorNotFound")]
    ActorNotFound(std::option::Option<String>),
}
impl std::fmt::Display for GetRelationshipsError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActorNotFound(msg) => {
                write!(f, "ActorNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
