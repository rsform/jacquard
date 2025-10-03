#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPreferencesParams {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetPreferencesOutput<'a> {
    #[serde(borrow)]
    pub preferences: crate::app_bsky::notification::Preferences<'a>,
}
