///Record declaring a 'block' relationship against another account. NOTE: blocks are public in Bluesky; see blog posts for details.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Block<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
