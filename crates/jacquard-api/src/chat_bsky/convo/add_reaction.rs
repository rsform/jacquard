#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddReactionInput<'a> {
    #[serde(borrow)]
    pub convo_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub message_id: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub value: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddReactionOutput<'a> {
    #[serde(borrow)]
    pub message: crate::chat_bsky::convo::MessageView<'a>,
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
pub enum AddReactionError<'a> {
    ///Indicates that the message has been deleted and reactions can no longer be added/removed.
    #[serde(rename = "ReactionMessageDeleted")]
    ReactionMessageDeleted(std::option::Option<String>),
    ///Indicates that the message has the maximum number of reactions allowed for a single user, and the requested reaction wasn't yet present. If it was already present, the request will not fail since it is idempotent.
    #[serde(rename = "ReactionLimitReached")]
    ReactionLimitReached(std::option::Option<String>),
    ///Indicates the value for the reaction is not acceptable. In general, this means it is not an emoji.
    #[serde(rename = "ReactionInvalidValue")]
    ReactionInvalidValue(std::option::Option<String>),
}
impl std::fmt::Display for AddReactionError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReactionMessageDeleted(msg) => {
                write!(f, "ReactionMessageDeleted")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::ReactionLimitReached(msg) => {
                write!(f, "ReactionLimitReached")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::ReactionInvalidValue(msg) => {
                write!(f, "ReactionInvalidValue")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Unknown(_) => write!(f, "Unknown error"),
        }
    }
}
