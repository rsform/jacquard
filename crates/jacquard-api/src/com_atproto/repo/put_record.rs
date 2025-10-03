#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutRecordInput<'a> {
    #[serde(borrow)]
    pub collection: jacquard_common::types::string::Nsid<'a>,
    #[serde(borrow)]
    pub record: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub repo: jacquard_common::types::ident::AtIdentifier<'a>,
    #[serde(borrow)]
    pub rkey: jacquard_common::types::string::RecordKey<
        jacquard_common::types::string::Rkey<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub swap_commit: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub swap_record: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub validate: std::option::Option<bool>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutRecordOutput<'a> {
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub commit: std::option::Option<crate::com_atproto::repo::CommitMeta<'a>>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub validation_status: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::open_union]
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    thiserror::Error,
    miette::Diagnostic
)]
#[serde(tag = "error", content = "message")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum PutRecordError<'a> {
    #[serde(rename = "InvalidSwap")]
    InvalidSwap(std::option::Option<String>),
}
impl std::fmt::Display for PutRecordError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSwap(msg) => {
                write!(f, "InvalidSwap")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
