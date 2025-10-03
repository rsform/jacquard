#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckHandleAvailabilityParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub birth_date: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub email: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckHandleAvailabilityOutput<'a> {
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(borrow)]
    pub result: CheckHandleAvailabilityOutputRecordResult<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum CheckHandleAvailabilityOutputRecordResult<'a> {}
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
pub enum CheckHandleAvailabilityError<'a> {
    ///An invalid email was provided.
    #[serde(rename = "InvalidEmail")]
    InvalidEmail(std::option::Option<String>),
}
impl std::fmt::Display for CheckHandleAvailabilityError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEmail(msg) => {
                write!(f, "InvalidEmail")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
///Indicates the provided handle is available.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResultAvailable<'a> {}
///Indicates the provided handle is unavailable and gives suggestions of available handles.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResultUnavailable<'a> {
    #[serde(borrow)]
    pub suggestions: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion<'a> {
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(borrow)]
    pub method: jacquard_common::CowStr<'a>,
}
