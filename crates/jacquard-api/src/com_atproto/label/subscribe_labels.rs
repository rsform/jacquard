#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Info<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub message: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Labels<'a> {
    #[serde(borrow)]
    pub labels: Vec<crate::com_atproto::label::Label<'a>>,
    pub seq: i64,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeLabelsParams {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub cursor: std::option::Option<i64>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum SubscribeLabelsMessage<'a> {
    #[serde(rename = "#labels")]
    Labels(Box<jacquard_common::types::value::Data<'a>>),
    #[serde(rename = "#info")]
    Info(Box<jacquard_common::types::value::Data<'a>>),
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
pub enum SubscribeLabelsError<'a> {
    #[serde(rename = "FutureCursor")]
    FutureCursor(std::option::Option<String>),
}
impl std::fmt::Display for SubscribeLabelsError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FutureCursor(msg) => {
                write!(f, "FutureCursor")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
