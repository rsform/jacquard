#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubjectStatusInput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub deactivated: std::option::Option<crate::com_atproto::admin::StatusAttr<'a>>,
    #[serde(borrow)]
    pub subject: UpdateSubjectStatusInputRecordSubject<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub takedown: std::option::Option<crate::com_atproto::admin::StatusAttr<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum UpdateSubjectStatusInputRecordSubject<'a> {
    #[serde(rename = "com.atproto.admin.defs#repoRef")]
    DefsRepoRef(Box<crate::com_atproto::admin::RepoRef<'a>>),
    #[serde(rename = "com.atproto.repo.strongRef")]
    StrongRef(Box<crate::com_atproto::repo::strong_ref::StrongRef<'a>>),
    #[serde(rename = "com.atproto.admin.defs#repoBlobRef")]
    DefsRepoBlobRef(Box<crate::com_atproto::admin::RepoBlobRef<'a>>),
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubjectStatusOutput<'a> {
    #[serde(borrow)]
    pub subject: UpdateSubjectStatusOutputRecordSubject<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub takedown: std::option::Option<crate::com_atproto::admin::StatusAttr<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum UpdateSubjectStatusOutputRecordSubject<'a> {
    #[serde(rename = "com.atproto.admin.defs#repoRef")]
    DefsRepoRef(Box<crate::com_atproto::admin::RepoRef<'a>>),
    #[serde(rename = "com.atproto.repo.strongRef")]
    StrongRef(Box<crate::com_atproto::repo::strong_ref::StrongRef<'a>>),
    #[serde(rename = "com.atproto.admin.defs#repoBlobRef")]
    DefsRepoBlobRef(Box<crate::com_atproto::admin::RepoBlobRef<'a>>),
}
