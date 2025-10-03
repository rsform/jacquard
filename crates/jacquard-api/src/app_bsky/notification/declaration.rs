///A declaration of the user's choices related to notifications that can be produced by them.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Declaration<'a> {
    #[serde(borrow)]
    pub allow_subscriptions: jacquard_common::CowStr<'a>,
}
