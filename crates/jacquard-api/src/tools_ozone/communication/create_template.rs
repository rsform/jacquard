#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateTemplateInput<'a> {
    #[serde(borrow)]
    pub content_markdown: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub created_by: std::option::Option<jacquard_common::types::string::Did<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub lang: std::option::Option<jacquard_common::types::string::Language>,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateTemplateOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::tools_ozone::communication::TemplateView<'a>,
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
pub enum CreateTemplateError<'a> {
    #[serde(rename = "DuplicateTemplateName")]
    DuplicateTemplateName(std::option::Option<String>),
}
impl std::fmt::Display for CreateTemplateError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateTemplateName(msg) => {
                write!(f, "DuplicateTemplateName")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
