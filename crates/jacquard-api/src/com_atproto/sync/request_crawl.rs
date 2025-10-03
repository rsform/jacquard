#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestCrawlInput<'a> {
    #[serde(borrow)]
    pub hostname: jacquard_common::CowStr<'a>,
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
pub enum RequestCrawlError<'a> {
    #[serde(rename = "HostBanned")]
    HostBanned(std::option::Option<String>),
}
impl std::fmt::Display for RequestCrawlError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostBanned(msg) => {
                write!(f, "HostBanned")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
