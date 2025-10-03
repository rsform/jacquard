#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Image<'a> {
    #[serde(borrow)]
    pub alt: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub aspect_ratio: std::option::Option<crate::app_bsky::embed::AspectRatio<'a>>,
    #[serde(borrow)]
    pub image: jacquard_common::types::blob::Blob<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Images<'a> {
    #[serde(borrow)]
    pub images: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    #[serde(borrow)]
    pub images: Vec<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewImage<'a> {
    #[serde(borrow)]
    pub alt: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub aspect_ratio: std::option::Option<crate::app_bsky::embed::AspectRatio<'a>>,
    #[serde(borrow)]
    pub fullsize: jacquard_common::types::string::Uri<'a>,
    #[serde(borrow)]
    pub thumb: jacquard_common::types::string::Uri<'a>,
}
