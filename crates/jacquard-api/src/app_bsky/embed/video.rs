#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Caption<'a> {
    #[serde(borrow)]
    pub file: jacquard_common::types::blob::Blob<'a>,
    pub lang: jacquard_common::types::string::Language,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Video<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub alt: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub aspect_ratio: std::option::Option<crate::app_bsky::embed::AspectRatio<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub captions: std::option::Option<Vec<jacquard_common::types::value::Data<'a>>>,
    #[serde(borrow)]
    pub video: jacquard_common::types::blob::Blob<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub alt: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub aspect_ratio: std::option::Option<crate::app_bsky::embed::AspectRatio<'a>>,
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(borrow)]
    pub playlist: jacquard_common::types::string::Uri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub thumbnail: std::option::Option<jacquard_common::types::string::Uri<'a>>,
}
