#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppPassword<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub privileged: std::option::Option<bool>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListAppPasswordsOutput<'a> {
    #[serde(borrow)]
    pub passwords: Vec<jacquard_common::types::value::Data<'a>>,
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
pub enum ListAppPasswordsError<'a> {
    #[serde(rename = "AccountTakedown")]
    AccountTakedown(std::option::Option<String>),
}
impl std::fmt::Display for ListAppPasswordsError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountTakedown(msg) => {
                write!(f, "AccountTakedown")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
