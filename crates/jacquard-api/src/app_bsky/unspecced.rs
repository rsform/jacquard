///Object used to store age assurance data in stash.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgeAssuranceEvent<'a> {
    #[serde(borrow)]
    pub attempt_id: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub complete_ip: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub complete_ua: std::option::Option<jacquard_common::CowStr<'a>>,
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub email: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub init_ip: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub init_ua: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub status: jacquard_common::CowStr<'a>,
}
///The computed state of the age assurance process, returned to the user in question on certain authenticated requests.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgeAssuranceState<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub last_initiated_at: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(borrow)]
    pub status: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonSearchActor<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonSearchPost<'a> {
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonSearchStarterPack<'a> {
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonTrend<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub category: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub dids: Vec<jacquard_common::types::string::Did<'a>>,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub link: jacquard_common::CowStr<'a>,
    pub post_count: i64,
    pub started_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub status: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub topic: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadItemBlocked<'a> {
    #[serde(borrow)]
    pub author: crate::app_bsky::feed::BlockedAuthor<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadItemNoUnauthenticated<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadItemNotFound<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadItemPost<'a> {
    pub hidden_by_threadgate: bool,
    pub more_parents: bool,
    pub more_replies: i64,
    pub muted_by_viewer: bool,
    pub op_thread: bool,
    #[serde(borrow)]
    pub post: crate::app_bsky::feed::PostView<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrendView<'a> {
    #[serde(borrow)]
    pub actors: Vec<crate::app_bsky::actor::ProfileViewBasic<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub category: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub link: jacquard_common::CowStr<'a>,
    pub post_count: i64,
    pub started_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub status: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub topic: jacquard_common::CowStr<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrendingTopic<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub display_name: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub link: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub topic: jacquard_common::CowStr<'a>,
}
pub mod get_age_assurance_state;
pub mod get_config;
pub mod get_onboarding_suggested_starter_packs;
pub mod get_onboarding_suggested_starter_packs_skeleton;
pub mod get_popular_feed_generators;
pub mod get_post_thread_other_v2;
pub mod get_post_thread_v2;
pub mod get_suggested_feeds;
pub mod get_suggested_feeds_skeleton;
pub mod get_suggested_starter_packs;
pub mod get_suggested_starter_packs_skeleton;
pub mod get_suggested_users;
pub mod get_suggested_users_skeleton;
pub mod get_suggestions_skeleton;
pub mod get_tagged_suggestions;
pub mod get_trending_topics;
pub mod get_trends;
pub mod get_trends_skeleton;
pub mod init_age_assurance;
pub mod search_actors_skeleton;
pub mod search_posts_skeleton;
pub mod search_starter_packs_skeleton;
