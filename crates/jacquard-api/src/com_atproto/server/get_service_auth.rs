#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetServiceAuthParams<'a> {
    #[serde(borrow)]
    pub aud: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub exp: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub lxm: std::option::Option<jacquard_common::types::string::Nsid<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetServiceAuthOutput<'a> {
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
pub enum GetServiceAuthError<'a> {
    ///Indicates that the requested expiration date is not a valid. May be in the past or may be reliant on the requested scopes.
    #[serde(rename = "BadExpiration")]
    BadExpiration(std::option::Option<String>),
}
impl std::fmt::Display for GetServiceAuthError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadExpiration(msg) => {
                write!(f, "BadExpiration")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
