#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordParams<'a> {
    #[serde(borrow)]
    pub collection: jacquard_common::types::string::Nsid<'a>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub rkey: jacquard_common::types::string::RecordKey<
        jacquard_common::types::string::Rkey<'a>,
    >,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordOutput<'a> {}
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
pub enum GetRecordError<'a> {
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(std::option::Option<String>),
    #[serde(rename = "RepoNotFound")]
    RepoNotFound(std::option::Option<String>),
    #[serde(rename = "RepoTakendown")]
    RepoTakendown(std::option::Option<String>),
    #[serde(rename = "RepoSuspended")]
    RepoSuspended(std::option::Option<String>),
    #[serde(rename = "RepoDeactivated")]
    RepoDeactivated(std::option::Option<String>),
}
impl std::fmt::Display for GetRecordError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RecordNotFound(msg) => {
                write!(f, "RecordNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::RepoNotFound(msg) => {
                write!(f, "RepoNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::RepoTakendown(msg) => {
                write!(f, "RepoTakendown")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::RepoSuspended(msg) => {
                write!(f, "RepoSuspended")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::RepoDeactivated(msg) => {
                write!(f, "RepoDeactivated")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
