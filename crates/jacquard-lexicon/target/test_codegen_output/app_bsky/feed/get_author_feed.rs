#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAuthorFeedParams<'a> {
    pub actor: jacquard_common::types::ident::AtIdentifier<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_pins: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAuthorFeedOutput<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<jacquard_common::CowStr<'a>>,
    pub feed: Vec<jacquard_common::types::value::Data<'a>>,
}
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
pub enum GetAuthorFeedError {
    #[serde(rename = "BlockedActor")]
    BlockedActor(Option<jacquard_common::CowStr<'static>>),
    #[serde(rename = "BlockedByActor")]
    BlockedByActor(Option<jacquard_common::CowStr<'static>>),
}
impl std::fmt::Display for GetAuthorFeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockedActor(msg) => {
                write!(f, "BlockedActor")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::BlockedByActor(msg) => {
                write!(f, "BlockedByActor")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}
