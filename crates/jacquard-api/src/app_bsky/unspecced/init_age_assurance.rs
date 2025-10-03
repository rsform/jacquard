#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitAgeAssuranceInput<'a> {
    #[serde(borrow)]
    pub country_code: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub email: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub language: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitAgeAssuranceOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::app_bsky::unspecced::AgeAssuranceState<'a>,
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
pub enum InitAgeAssuranceError<'a> {
    #[serde(rename = "InvalidEmail")]
    InvalidEmail(std::option::Option<String>),
    #[serde(rename = "DidTooLong")]
    DidTooLong(std::option::Option<String>),
    #[serde(rename = "InvalidInitiation")]
    InvalidInitiation(std::option::Option<String>),
}
impl std::fmt::Display for InitAgeAssuranceError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEmail(msg) => {
                write!(f, "InvalidEmail")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::DidTooLong(msg) => {
                write!(f, "DidTooLong")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::InvalidInitiation(msg) => {
                write!(f, "InvalidInitiation")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
