#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySubscription<'a> {
    pub post: bool,
    pub reply: bool,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatPreference<'a> {
    #[serde(borrow)]
    pub include: jacquard_common::CowStr<'a>,
    pub push: bool,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FilterablePreference<'a> {
    #[serde(borrow)]
    pub include: jacquard_common::CowStr<'a>,
    pub list: bool,
    pub push: bool,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Preference<'a> {
    pub list: bool,
    pub push: bool,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Preferences<'a> {
    #[serde(borrow)]
    pub chat: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub follow: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub like: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub like_via_repost: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub mention: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub quote: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub reply: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub repost: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub repost_via_repost: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub starterpack_joined: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub subscribed_post: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub unverified: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub verified: jacquard_common::types::value::Data<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecordDeleted<'a> {}
///Object used to store activity subscription data in stash.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubjectActivitySubscription<'a> {
    #[serde(borrow)]
    pub activity_subscription: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
pub mod declaration;
pub mod get_preferences;
pub mod get_unread_count;
pub mod list_activity_subscriptions;
pub mod list_notifications;
pub mod put_activity_subscription;
pub mod put_preferences;
pub mod put_preferences_v2;
pub mod register_push;
pub mod unregister_push;
pub mod update_seen;
