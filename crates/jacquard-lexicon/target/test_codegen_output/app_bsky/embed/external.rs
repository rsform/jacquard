#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct External<'a> {
    pub description: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<jacquard_common::types::blob::Blob<'a>>,
    pub title: jacquard_common::CowStr<'a>,
    pub uri: jacquard_common::types::string::Uri<'a>,
}
///A representation of some externally linked content (eg, a URL and 'card'), embedded in a Bluesky record (eg, a post).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct External<'a> {
    pub external: jacquard_common::types::value::Data<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    pub external: jacquard_common::types::value::Data<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewExternal<'a> {
    pub description: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<jacquard_common::types::string::Uri<'a>>,
    pub title: jacquard_common::CowStr<'a>,
    pub uri: jacquard_common::types::string::Uri<'a>,
}
