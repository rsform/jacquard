#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateView<'a> {
    #[serde(borrow)]
    pub content_markdown: jacquard_common::CowStr<'a>,
    pub created_at: jacquard_common::types::string::Datetime,
    pub disabled: bool,
    #[serde(borrow)]
    pub id: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub lang: std::option::Option<jacquard_common::types::string::Language>,
    #[serde(borrow)]
    pub last_updated_by: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subject: std::option::Option<jacquard_common::CowStr<'a>>,
    pub updated_at: jacquard_common::types::string::Datetime,
}
pub mod create_template;
pub mod delete_template;
pub mod list_templates;
pub mod update_template;
