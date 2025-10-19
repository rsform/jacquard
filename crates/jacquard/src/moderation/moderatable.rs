use super::{LabelerDefs, ModerationDecision, ModerationPrefs, moderate};
use jacquard_common::types::string::Did;

/// Trait for composite types that contain multiple labeled items
///
/// Types like `FeedViewPost` contain several pieces that each have their own labels
/// (post, author, reply chain, embeds, etc.). This trait allows them to return
/// moderation decisions for all their parts, tagged with identifiers so consumers
/// can handle each part appropriately.
///
/// # Example
///
/// ```ignore
/// # use jacquard::moderation::*;
/// # use jacquard_api::app_bsky::feed::FeedViewPost;
/// # fn example(feed_post: &FeedViewPost<'_>, prefs: &ModerationPrefs<'_>, defs: &LabelerDefs<'_>) {
/// for (tag, decision) in feed_post.moderate_all(prefs, defs, &[]) {
///     match tag {
///         "post" if decision.filter => println!("Hide post content"),
///         "author" if decision.filter => println!("Hide author info"),
///         _ => {}
///     }
/// }
/// # }
/// ```
pub trait Moderateable<'a> {
    /// Apply moderation to all labeled parts of this item
    ///
    /// Returns a vector of (tag, decision) tuples where the tag identifies
    /// which part of the composite item the decision applies to.
    fn moderate_all(
        &'a self,
        prefs: &ModerationPrefs<'_>,
        defs: &LabelerDefs<'_>,
        accepted_labelers: &[Did<'_>],
    ) -> Vec<(&'static str, ModerationDecision)>;
}

// Implementations for common Bluesky types
#[cfg(feature = "api_bluesky")]
mod bluesky_impls {
    use super::*;
    use jacquard_api::app_bsky::feed::{FeedViewPost, ReplyRefParent, ReplyRefRoot};

    impl<'a> Moderateable<'a> for FeedViewPost<'a> {
        fn moderate_all(
            &'a self,
            prefs: &ModerationPrefs<'_>,
            defs: &LabelerDefs<'_>,
            accepted_labelers: &[Did<'_>],
        ) -> Vec<(&'static str, ModerationDecision)> {
            let mut decisions = vec![
                ("post", moderate(&self.post, prefs, defs, accepted_labelers)),
                (
                    "author",
                    moderate(&self.post.author, prefs, defs, accepted_labelers),
                ),
            ];

            // Add reply chain decisions if present
            if let Some(reply) = &self.reply {
                // Parent post and author
                if let ReplyRefParent::PostView(parent) = &reply.parent {
                    decisions.push((
                        "reply_parent",
                        moderate(&**parent, prefs, defs, accepted_labelers),
                    ));
                    decisions.push((
                        "reply_parent_author",
                        moderate(&parent.author, prefs, defs, accepted_labelers),
                    ));
                }

                // Root post and author
                if let ReplyRefRoot::PostView(root) = &reply.root {
                    decisions.push((
                        "reply_root",
                        moderate(&**root, prefs, defs, accepted_labelers),
                    ));
                    decisions.push((
                        "reply_root_author",
                        moderate(&root.author, prefs, defs, accepted_labelers),
                    ));
                }

                // Grandparent author
                if let Some(grandparent_author) = &reply.grandparent_author {
                    decisions.push((
                        "reply_grandparent_author",
                        moderate(grandparent_author, prefs, defs, accepted_labelers),
                    ));
                }
            }

            // TODO: handle embeds (quote posts, external links with metadata, etc.)
            // if let Some(embed) = &self.post.embed {
            //     match embed {
            //         PostViewEmbedRecord(record) => { ... }
            //         ...
            //     }
            // }

            decisions
        }
    }
}
