#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListMissingBlobsParams<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub limit: std::option::Option<i64>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListMissingBlobsOutput<'a> {
    #[serde(borrow)]
    pub blobs: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cursor: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecordBlob<'a> {
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(borrow)]
    pub record_uri: jacquard_common::types::string::AtUri<'a>,
}
