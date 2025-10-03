#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Caption<'a> {
    pub file: jacquard_common::types::blob::Blob<'a>,
    pub lang: jacquard_common::types::string::Language,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Video<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captions: Option<Vec<jacquard_common::types::value::Data<'a>>>,
    pub video: jacquard_common::types::blob::Blob<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<jacquard_common::types::value::Data<'a>>,
    pub cid: jacquard_common::types::string::Cid<'a>,
    pub playlist: jacquard_common::types::string::Uri<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<jacquard_common::types::string::Uri<'a>>,
}
