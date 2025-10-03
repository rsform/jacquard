#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetReporterStatsParams<'a> {
    #[serde(borrow)]
    pub dids: Vec<jacquard_common::types::string::Did<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetReporterStatsOutput<'a> {
    #[serde(borrow)]
    pub stats: Vec<crate::tools_ozone::moderation::ReporterStats<'a>>,
}
