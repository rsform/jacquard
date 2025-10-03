#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutPreferencesInput<'a> {
    #[serde(borrow)]
    pub preferences: crate::app_bsky::actor::Preferences<'a>,
}
