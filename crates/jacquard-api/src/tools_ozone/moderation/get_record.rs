#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::tools_ozone::moderation::RecordViewDetail<'a>,
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
pub enum GetRecordError<'a> {
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(std::option::Option<String>),
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
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
