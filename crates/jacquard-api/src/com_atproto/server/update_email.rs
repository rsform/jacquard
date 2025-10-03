#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEmailInput<'a> {
    #[serde(borrow)]
    pub email: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub email_auth_factor: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub token: std::option::Option<jacquard_common::CowStr<'a>>,
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
pub enum UpdateEmailError<'a> {
    #[serde(rename = "ExpiredToken")]
    ExpiredToken(std::option::Option<String>),
    #[serde(rename = "InvalidToken")]
    InvalidToken(std::option::Option<String>),
    #[serde(rename = "TokenRequired")]
    TokenRequired(std::option::Option<String>),
}
impl std::fmt::Display for UpdateEmailError<'_> {
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
            Self::TokenRequired(msg) => {
                write!(f, "TokenRequired")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
