#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutPreferencesV2Input<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub chat: std::option::Option<crate::app_bsky::notification::ChatPreference<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub follow: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub like: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub like_via_repost: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub mention: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub quote: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reply: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub repost: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub repost_via_repost: std::option::Option<
        crate::app_bsky::notification::FilterablePreference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub starterpack_joined: std::option::Option<
        crate::app_bsky::notification::Preference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub subscribed_post: std::option::Option<
        crate::app_bsky::notification::Preference<'a>,
    >,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub unverified: std::option::Option<crate::app_bsky::notification::Preference<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub verified: std::option::Option<crate::app_bsky::notification::Preference<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutPreferencesV2Output<'a> {
    #[serde(borrow)]
    pub preferences: crate::app_bsky::notification::Preferences<'a>,
}
