#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchActorsSkeletonParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub limit: std::option::Option<i64>,
    #[serde(borrow)]
    pub q: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub typeahead: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchActorsSkeletonOutput<'a> {
    #[serde(borrow)]
    pub actors: Vec<crate::app_bsky::unspecced::SkeletonSearchActor<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub hits_total: std::option::Option<i64>,
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
pub enum SearchActorsSkeletonError<'a> {
    #[serde(rename = "BadQueryString")]
    BadQueryString(std::option::Option<String>),
}
impl std::fmt::Display for SearchActorsSkeletonError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadQueryString(msg) => {
                write!(f, "BadQueryString")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
