#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetJobStatusParams<'a> {
    #[serde(borrow)]
    pub job_id: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetJobStatusOutput<'a> {
    #[serde(borrow)]
    pub job_status: crate::app_bsky::video::JobStatus<'a>,
}
