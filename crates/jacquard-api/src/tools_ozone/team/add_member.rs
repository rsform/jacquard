#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberInput<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub role: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberOutput<'a> {
    #[serde(flatten)]
    #[serde(borrow)]
    pub value: crate::tools_ozone::team::Member<'a>,
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
pub enum AddMemberError<'a> {
    ///Member already exists in the team.
    #[serde(rename = "MemberAlreadyExists")]
    MemberAlreadyExists(std::option::Option<String>),
}
impl std::fmt::Display for AddMemberError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MemberAlreadyExists(msg) => {
                write!(f, "MemberAlreadyExists")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
