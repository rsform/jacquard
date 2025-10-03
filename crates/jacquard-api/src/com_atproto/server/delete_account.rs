#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountInput<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub password: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub token: jacquard_common::CowStr<'a>,
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
pub enum DeleteAccountError<'a> {
    #[serde(rename = "ExpiredToken")]
    ExpiredToken(std::option::Option<String>),
    #[serde(rename = "InvalidToken")]
    InvalidToken(std::option::Option<String>),
}
impl std::fmt::Display for DeleteAccountError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpiredToken(msg) => {
                write!(f, "ExpiredToken")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::InvalidToken(msg) => {
                write!(f, "InvalidToken")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
