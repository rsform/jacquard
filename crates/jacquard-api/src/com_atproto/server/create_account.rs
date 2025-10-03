#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub did: std::option::Option<jacquard_common::types::string::Did<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub email: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub invite_code: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub password: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub plc_op: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub recovery_key: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub verification_code: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub verification_phone: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountOutput<'a> {
    #[serde(borrow)]
    pub access_jwt: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub did_doc: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(borrow)]
    pub refresh_jwt: jacquard_common::CowStr<'a>,
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
pub enum CreateAccountError<'a> {
    #[serde(rename = "InvalidHandle")]
    InvalidHandle(std::option::Option<String>),
    #[serde(rename = "InvalidPassword")]
    InvalidPassword(std::option::Option<String>),
    #[serde(rename = "InvalidInviteCode")]
    InvalidInviteCode(std::option::Option<String>),
    #[serde(rename = "HandleNotAvailable")]
    HandleNotAvailable(std::option::Option<String>),
    #[serde(rename = "UnsupportedDomain")]
    UnsupportedDomain(std::option::Option<String>),
    #[serde(rename = "UnresolvableDid")]
    UnresolvableDid(std::option::Option<String>),
    #[serde(rename = "IncompatibleDidDoc")]
    IncompatibleDidDoc(std::option::Option<String>),
}
impl std::fmt::Display for CreateAccountError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHandle(msg) => {
                write!(f, "InvalidHandle")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::InvalidPassword(msg) => {
                write!(f, "InvalidPassword")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::InvalidInviteCode(msg) => {
                write!(f, "InvalidInviteCode")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::HandleNotAvailable(msg) => {
                write!(f, "HandleNotAvailable")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::UnsupportedDomain(msg) => {
                write!(f, "UnsupportedDomain")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::UnresolvableDid(msg) => {
                write!(f, "UnresolvableDid")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::IncompatibleDidDoc(msg) => {
                write!(f, "IncompatibleDidDoc")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
