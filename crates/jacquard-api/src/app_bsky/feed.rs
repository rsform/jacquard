#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockedAuthor<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<crate::app_bsky::actor::ViewerState<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockedPost<'a> {
    #[serde(borrow)]
    pub author: jacquard_common::types::value::Data<'a>,
    pub blocked: bool,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedViewPost<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub feed_context: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub post: jacquard_common::types::value::Data<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reason: std::option::Option<FeedViewPostRecordReason<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reply: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub req_id: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum FeedViewPostRecordReason<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorView<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub accepts_interactions: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub avatar: std::option::Option<jacquard_common::types::string::Uri<'a>>,
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub content_mode: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub creator: crate::app_bsky::actor::ProfileView<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub description_facets: std::option::Option<
        Vec<crate::app_bsky::richtext::facet::Facet<'a>>,
    >,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<Vec<crate::com_atproto::label::Label<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub like_count: std::option::Option<i64>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorViewerState<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub like: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Interaction<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub event: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub feed_context: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub item: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub req_id: std::option::Option<jacquard_common::CowStr<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotFoundPost<'a> {
    pub not_found: bool,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PostView<'a> {
    #[serde(borrow)]
    pub author: crate::app_bsky::actor::ProfileViewBasic<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub bookmark_count: std::option::Option<i64>,
    #[serde(borrow)]
    pub cid: jacquard_common::types::string::Cid<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub embed: std::option::Option<PostViewRecordEmbed<'a>>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub labels: std::option::Option<Vec<crate::com_atproto::label::Label<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub like_count: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub quote_count: std::option::Option<i64>,
    #[serde(borrow)]
    pub record: jacquard_common::types::value::Data<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub reply_count: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub repost_count: std::option::Option<i64>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub threadgate: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub viewer: std::option::Option<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum PostViewRecordEmbed<'a> {
    #[serde(rename = "app.bsky.embed.images#view")]
    ImagesView(Box<crate::app_bsky::embed::images::View<'a>>),
    #[serde(rename = "app.bsky.embed.video#view")]
    VideoView(Box<crate::app_bsky::embed::video::View<'a>>),
    #[serde(rename = "app.bsky.embed.external#view")]
    ExternalView(Box<crate::app_bsky::embed::external::View<'a>>),
    #[serde(rename = "app.bsky.embed.record#view")]
    RecordView(Box<crate::app_bsky::embed::record::View<'a>>),
    #[serde(rename = "app.bsky.embed.recordWithMedia#view")]
    RecordWithMediaView(Box<crate::app_bsky::embed::record_with_media::View<'a>>),
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReasonPin<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReasonRepost<'a> {
    #[serde(borrow)]
    pub by: crate::app_bsky::actor::ProfileViewBasic<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    pub indexed_at: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub uri: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRef<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub grandparent_author: std::option::Option<
        crate::app_bsky::actor::ProfileViewBasic<'a>,
    >,
    #[serde(borrow)]
    pub parent: ReplyRefRecordParent<'a>,
    #[serde(borrow)]
    pub root: ReplyRefRecordRoot<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ReplyRefRecordParent<'a> {}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ReplyRefRecordRoot<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonFeedPost<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub feed_context: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub post: jacquard_common::types::string::AtUri<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub reason: std::option::Option<SkeletonFeedPostRecordReason<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum SkeletonFeedPostRecordReason<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonReasonPin<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonReasonRepost<'a> {
    #[serde(borrow)]
    pub repost: jacquard_common::types::string::AtUri<'a>,
}
///Metadata about this post within the context of the thread it is in.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadContext<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub root_author_like: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadViewPost<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub parent: std::option::Option<ThreadViewPostRecordParent<'a>>,
    #[serde(borrow)]
    pub post: jacquard_common::types::value::Data<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub replies: std::option::Option<Vec<jacquard_common::types::value::Data<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub thread_context: std::option::Option<jacquard_common::types::value::Data<'a>>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ThreadViewPostRecordParent<'a> {}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadgateView<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub lists: std::option::Option<Vec<crate::app_bsky::graph::ListViewBasic<'a>>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub record: std::option::Option<jacquard_common::types::value::Data<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub uri: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
}
///Metadata about the requesting account's relationship with the subject content. Only has meaningful content for authed requests.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewerState<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub bookmarked: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub embedding_disabled: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub like: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub pinned: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub reply_disabled: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub repost: std::option::Option<jacquard_common::types::string::AtUri<'a>>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub thread_muted: std::option::Option<bool>,
}
pub mod describe_feed_generator;
pub mod generator;
pub mod get_actor_feeds;
pub mod get_actor_likes;
pub mod get_author_feed;
pub mod get_feed;
pub mod get_feed_generator;
pub mod get_feed_generators;
pub mod get_feed_skeleton;
pub mod get_likes;
pub mod get_list_feed;
pub mod get_post_thread;
pub mod get_posts;
pub mod get_quotes;
pub mod get_reposted_by;
pub mod get_suggested_feeds;
pub mod get_timeline;
pub mod like;
pub mod post;
pub mod postgate;
pub mod repost;
pub mod search_posts;
pub mod send_interactions;
pub mod threadgate;
