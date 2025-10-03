#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRuleInput<'a> {
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
pub struct UpdateRuleOutput<'a> {
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
pub enum UpdateRuleError<'a> {
    ///No active rule found for this URL/domain
    #[serde(rename = "RuleNotFound")]
    RuleNotFound(std::option::Option<String>),
}
impl std::fmt::Display for UpdateRuleError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RuleNotFound(msg) => {
                write!(f, "RuleNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
