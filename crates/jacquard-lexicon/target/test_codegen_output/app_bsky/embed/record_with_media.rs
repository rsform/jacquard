#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecordWithMedia<'a> {
    pub media: RecordMedia<'a>,
    pub record: test_generated::app_bsky::embed::Record<'a>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View<'a> {
    pub media: RecordMedia<'a>,
    pub record: test_generated::app_bsky::embed::View<'a>,
}
