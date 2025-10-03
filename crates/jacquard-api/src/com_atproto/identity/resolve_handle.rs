#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolveHandleParams<'a> {
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolveHandleOutput<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
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
pub enum ResolveHandleError<'a> {
    ///The resolution process confirmed that the handle does not resolve to any DID.
    #[serde(rename = "HandleNotFound")]
    HandleNotFound(std::option::Option<String>),
}
impl std::fmt::Display for ResolveHandleError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandleNotFound(msg) => {
                write!(f, "HandleNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
