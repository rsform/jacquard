#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetTaggedSuggestionsParams {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetTaggedSuggestionsOutput<'a> {
    #[serde(borrow)]
    pub suggestions: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion<'a> {
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Uri<'a>,
    #[serde(borrow)]
    pub subject_type: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub tag: jacquard_common::CowStr<'a>,
}
