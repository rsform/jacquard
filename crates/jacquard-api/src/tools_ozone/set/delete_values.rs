#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteValuesInput<'a> {
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub values: Vec<jacquard_common::CowStr<'a>>,
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
pub enum DeleteValuesError<'a> {
    ///set with the given name does not exist
    #[serde(rename = "SetNotFound")]
    SetNotFound(std::option::Option<String>),
}
impl std::fmt::Display for DeleteValuesError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetNotFound(msg) => {
                write!(f, "SetNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
