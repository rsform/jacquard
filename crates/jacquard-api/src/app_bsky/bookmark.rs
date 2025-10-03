///Object used to store bookmark data in stash.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark<'a> {
    #[serde(borrow)]
    pub subject: crate::com_atproto::repo::strong_ref::StrongRef<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkView<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub created_at: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(borrow)]
    pub item: BookmarkViewRecordItem<'a>,
    #[serde(borrow)]
    pub subject: crate::com_atproto::repo::strong_ref::StrongRef<'a>,
}
#[jacquard_derive::open_union]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum BookmarkViewRecordItem<'a> {
    #[serde(rename = "app.bsky.feed.defs#blockedPost")]
    DefsBlockedPost(Box<crate::app_bsky::feed::BlockedPost<'a>>),
    #[serde(rename = "app.bsky.feed.defs#notFoundPost")]
    DefsNotFoundPost(Box<crate::app_bsky::feed::NotFoundPost<'a>>),
    #[serde(rename = "app.bsky.feed.defs#postView")]
    DefsPostView(Box<crate::app_bsky::feed::PostView<'a>>),
}
pub mod create_bookmark;
pub mod delete_bookmark;
pub mod get_bookmarks;
