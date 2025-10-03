#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub blob: std::option::Option<jacquard_common::types::blob::Blob<'a>>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub error: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub job_id: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub message: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub progress: std::option::Option<i64>,
    #[serde(borrow)]
    pub state: jacquard_common::CowStr<'a>,
}
pub mod get_job_status;
pub mod get_upload_limits;
pub mod upload_video;
