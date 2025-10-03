#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddRuleInput<'a> {
    #[serde(borrow)]
    pub action: crate::tools_ozone::safelink::ActionType<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub comment: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub created_by: std::option::Option<jacquard_common::types::string::Did<'a>>,
    #[serde(borrow)]
    pub pattern: crate::tools_ozone::safelink::PatternType<'a>,
    #[serde(borrow)]
    pub reason: crate::tools_ozone::safelink::ReasonType<'a>,
    #[serde(borrow)]
    pub url: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddRuleOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::tools_ozone::safelink::Event<'a>,
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
pub enum AddRuleError<'a> {
    ///The provided URL is invalid
    #[serde(rename = "InvalidUrl")]
    InvalidUrl(std::option::Option<String>),
    ///A rule for this URL/domain already exists
    #[serde(rename = "RuleAlreadyExists")]
    RuleAlreadyExists(std::option::Option<String>),
}
impl std::fmt::Display for AddRuleError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(msg) => {
                write!(f, "InvalidUrl")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::RuleAlreadyExists(msg) => {
                write!(f, "RuleAlreadyExists")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
