///width:height represents an aspect ratio. It may be approximate, and may not correspond to absolute dimensions in any given unit.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AspectRatio<'a> {
    pub height: i64,
    pub width: i64,
}
pub mod external;
pub mod images;
pub mod record;
pub mod record_with_media;
pub mod video;
