#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDidParams<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDidOutput<'a> {
    #[serde(borrow)]
    pub did_doc: jacquard_common::types::value::Data<'a>,
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
pub enum ResolveDidError<'a> {
    ///The DID resolution process confirmed that there is no current DID.
    #[serde(rename = "DidNotFound")]
    DidNotFound(std::option::Option<String>),
    ///The DID previously existed, but has been deactivated.
    #[serde(rename = "DidDeactivated")]
    DidDeactivated(std::option::Option<String>),
}
impl std::fmt::Display for ResolveDidError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DidNotFound(msg) => {
                write!(f, "DidNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::DidDeactivated(msg) => {
                write!(f, "DidDeactivated")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
