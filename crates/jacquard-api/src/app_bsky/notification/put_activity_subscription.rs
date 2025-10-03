#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutActivitySubscriptionInput<'a> {
    #[serde(borrow)]
    pub activity_subscription: crate::app_bsky::notification::ActivitySubscription<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutActivitySubscriptionOutput<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub activity_subscription: std::option::Option<
        crate::app_bsky::notification::ActivitySubscription<'a>,
    >,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
