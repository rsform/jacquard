#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Image<'a> {
    pub alt: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<jacquard_common::types::value::Data<'a>>,
    pub image: jacquard_common::types::blob::Blob<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Images<'a> {
    pub images: Vec<jacquard_common::types::value::Data<'a>>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    pub images: Vec<jacquard_common::types::value::Data<'a>>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewImage<'a> {
    pub alt: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<jacquard_common::types::value::Data<'a>>,
    pub fullsize: jacquard_common::types::string::Uri<'a>,
    pub thumb: jacquard_common::types::string::Uri<'a>,
}
