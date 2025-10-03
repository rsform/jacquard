#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListTemplatesOutput<'a> {
    #[serde(borrow)]
    pub communication_templates: Vec<
        crate::tools_ozone::communication::TemplateView<'a>,
    >,
}
