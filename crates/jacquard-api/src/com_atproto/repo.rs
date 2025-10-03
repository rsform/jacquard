#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitMeta<'a> {
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    pub rev: jacquard_common::types::string::Tid,
}
pub mod apply_writes;
pub mod create_record;
pub mod delete_record;
pub mod describe_repo;
pub mod get_record;
pub mod import_repo;
pub mod list_missing_blobs;
pub mod list_records;
pub mod put_record;
pub mod strong_ref;
pub mod upload_blob;
