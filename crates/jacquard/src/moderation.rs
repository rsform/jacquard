//! Moderation
//!
//! This is an attempt to semi-generalize the Bluesky moderation system. It avoids
//! depending on their lexicons as much as reasonably possible. This works via a
//! trait, [`Labeled`][crate::moderation::Labeled], which represents things that have labels for moderation
//! applied to them. This way the moderation application functions can operate
//! primarily via the trait, and are thus generic over lexicon types, and are
//! easy to use with your own types.
//!
//! For more complex types which might have labels applied to components,
//! there is the [`Moderateable`][crate::moderation::Moderateable] trait. A mostly complete implementation for
//! `FeedViewPost` is available for reference. The trait method outputs a `Vec`
//! of tuples, where the first element is a string tag and the second is the
//! moderation decision for the tagged element. This lets application developers
//! change behaviour based on what part of the content got a label. The functions
//! mostly match Bluesky behaviour (respecting "!hide", and such) by default.
//!
//! I've taken the time to go through the generated API bindings and implement
//! the [`Labeled`][crate::moderation::Labeled] trait for a number of types. It's a fairly easy trait to
//! implement, just not really automatable.
//!
//!
//! # Example
//!
//! ```ignore
//! # use jacquard::moderation::*;
//! # use jacquard_api::app_bsky::feed::PostView;
//! # fn example(post: &PostView<'_>, prefs: &ModerationPrefs<'_>, defs: &LabelerDefs<'_>) {
//! let decision = moderate(post, prefs, defs, &[]);
//! if decision.filter {
//!     // hide the post
//! } else if decision.blur != Blur::None {
//!     // show with blur
//! }
//! # }
//! ```

mod decision;
#[cfg(feature = "api")]
mod fetch;
mod labeled;
mod moderatable;
mod types;

#[cfg(test)]
mod tests;

pub use decision::{ModerationIterExt, moderate, moderate_all};
#[cfg(feature = "api")]
pub use fetch::{fetch_labeled_record, fetch_labels};
#[cfg(feature = "api_bluesky")]
pub use fetch::{fetch_labeler_defs, fetch_labeler_defs_direct};
pub use labeled::{Labeled, LabeledRecord};
pub use moderatable::{ModeratableIterExt, Moderateable};
pub use types::{
    Blur, LabelCause, LabelPref, LabelTarget, LabelerDefs, ModerationDecision, ModerationPrefs,
};
