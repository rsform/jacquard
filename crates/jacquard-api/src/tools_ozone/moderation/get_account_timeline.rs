#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountTimelineParams<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountTimelineOutput<'a> {
    #[serde(borrow)]
    pub timeline: Vec<jacquard_common::types::value::Data<'a>>,
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
pub enum GetAccountTimelineError<'a> {
    #[serde(rename = "RepoNotFound")]
    RepoNotFound(std::option::Option<String>),
}
impl std::fmt::Display for GetAccountTimelineError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RepoNotFound(msg) => {
                write!(f, "RepoNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem<'a> {
    #[serde(borrow)]
    pub day: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub summary: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItemSummary<'a> {
    pub count: i64,
    #[serde(borrow)]
    pub event_subject_type: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub event_type: jacquard_common::CowStr<'a>,
}
