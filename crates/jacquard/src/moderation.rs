//! Moderation decision making for AT Protocol content
//!
//! This module provides protocol-agnostic moderation logic for applying label-based
//! content filtering. It takes labels from various sources (labeler services, self-labels)
//! and user preferences to produce moderation decisions.
//!
//! # Core Concepts
//!
//! - **Labels**: Metadata tags applied to content by labelers or authors (see [`Label`](jacquard_api::com_atproto::label::Label))
//! - **Preferences**: User-configured responses to specific label values (hide, warn, ignore)
//! - **Definitions**: Labeler-provided metadata about what labels mean and how they should be displayed
//! - **Decisions**: The output of moderation logic indicating what actions to take
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
#[cfg(feature = "api_bluesky")]
mod fetch;
mod labeled;
mod moderatable;
mod types;

#[cfg(test)]
mod tests;

pub use decision::{ModerationIterExt, moderate, moderate_all};
#[cfg(feature = "api_bluesky")]
pub use fetch::{fetch_labeler_defs, fetch_labeler_defs_direct};
pub use labeled::Labeled;
pub use moderatable::Moderateable;
pub use types::{
    Blur, LabelCause, LabelPref, LabelTarget, LabelerDefs, ModerationDecision, ModerationPrefs,
};
