#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct External<'a> {
    #[serde(borrow)]
    pub description: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub thumb: std::option::Option<jacquard_common::types::blob::Blob<'a>>,
    #[serde(borrow)]
    pub title: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::Uri<'a>,
}
///A representation of some externally linked content (eg, a URL and 'card'), embedded in a Bluesky record (eg, a post).
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRecord<'a> {
    #[serde(borrow)]
    pub external: jacquard_common::types::value::Data<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    #[serde(borrow)]
    pub external: jacquard_common::types::value::Data<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewExternal<'a> {
    #[serde(borrow)]
    pub description: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub thumb: std::option::Option<jacquard_common::types::string::Uri<'a>>,
    #[serde(borrow)]
    pub title: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::Uri<'a>,
}
