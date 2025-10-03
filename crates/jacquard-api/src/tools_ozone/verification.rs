///Verification data for the associated subject.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationView<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(borrow)]
    pub issuer: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub issuer_profile: std::option::Option<VerificationViewRecordIssuerProfile<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub issuer_repo: std::option::Option<VerificationViewRecordIssuerRepo<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub revoke_reason: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub revoked_at: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub revoked_by: std::option::Option<jacquard_common::types::string::Did<'a>>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subject_profile: std::option::Option<VerificationViewRecordSubjectProfile<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subject_repo: std::option::Option<VerificationViewRecordSubjectRepo<'a>>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum VerificationViewRecordIssuerProfile<'a> {}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum VerificationViewRecordIssuerRepo<'a> {
    #[serde(rename = "tools.ozone.moderation.defs#repoViewDetail")]
    DefsRepoViewDetail(Box<crate::tools_ozone::moderation::RepoViewDetail<'a>>),
    #[serde(rename = "tools.ozone.moderation.defs#repoViewNotFound")]
    DefsRepoViewNotFound(Box<crate::tools_ozone::moderation::RepoViewNotFound<'a>>),
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum VerificationViewRecordSubjectProfile<'a> {}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum VerificationViewRecordSubjectRepo<'a> {
    #[serde(rename = "tools.ozone.moderation.defs#repoViewDetail")]
    DefsRepoViewDetail(Box<crate::tools_ozone::moderation::RepoViewDetail<'a>>),
    #[serde(rename = "tools.ozone.moderation.defs#repoViewNotFound")]
    DefsRepoViewNotFound(Box<crate::tools_ozone::moderation::RepoViewNotFound<'a>>),
}
pub mod grant_verifications;
pub mod list_verifications;
pub mod revoke_verifications;
