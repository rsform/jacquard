///A declaration of a Bluesky chat account.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Declaration<'a> {
    #[serde(borrow)]
    pub allow_incoming: jacquard_common::CowStr<'a>,
}
