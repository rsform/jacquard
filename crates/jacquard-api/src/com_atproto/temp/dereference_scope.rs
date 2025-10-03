#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DereferenceScopeParams<'a> {
    #[serde(borrow)]
    pub scope: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DereferenceScopeOutput<'a> {
    #[serde(borrow)]
    pub scope: jacquard_common::CowStr<'a>,
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
pub enum DereferenceScopeError<'a> {
    ///An invalid scope reference was provided.
    #[serde(rename = "InvalidScopeReference")]
    InvalidScopeReference(std::option::Option<String>),
}
impl std::fmt::Display for DereferenceScopeError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScopeReference(msg) => {
                write!(f, "InvalidScopeReference")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
