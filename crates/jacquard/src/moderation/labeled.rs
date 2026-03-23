use jacquard_api::com_atproto::label::{Label, SelfLabels};
use jacquard_common::bos::{BosStr, DefaultStr};

/// Trait for content that has labels attached
///
/// Implemented by types that can be moderated based on their labels.
/// This includes both labels from labeler services and self-labels applied by authors.
///
/// The type parameter `S` is the backing string type used by the label types.
/// In most concrete cases this will be `DefaultStr` (`SmolStr`).
pub trait Labeled<S: BosStr = DefaultStr> {
    /// Get the labels applied to this content by labeler services
    fn labels(&self) -> &[Label<S>];

    /// Get self-labels applied by the content author
    fn self_labels(&self) -> Option<SelfLabels<S>> {
        None
    }
}

/// Record with applied labels
///
/// Exists as a bare minimum RecordView type primarily for testing/demonstration.
pub struct LabeledRecord<S: BosStr = DefaultStr, C = ()> {
    /// The record we grabbed labels for
    pub record: C,
    /// The labels applied to the record
    pub labels: Vec<Label<S>>,
}

impl<S: BosStr, C> Labeled<S> for LabeledRecord<S, C> {
    fn labels(&self) -> &[Label<S>] {
        &self.labels
    }
}

// Implementations for common Bluesky types
#[cfg(feature = "api_bluesky")]
mod bluesky_impls {
    use super::*;
    use jacquard_api::app_bsky::{
        actor::{ProfileView, ProfileViewBasic, ProfileViewDetailed, profile::Profile},
        feed::{PostView, generator::Generator, post::Post},
        graph::{ListView, list::List},
        labeler::service::Service,
        notification::list_notifications::Notification,
    };
    use jacquard_common::from_data;

    impl<S: BosStr> Labeled<S> for PostView<S>
    where
        S: for<'de> serde::Deserialize<'de> + for<'de> core::convert::From<&'de str>,
    {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            let post = from_data::<Post<S>, S>(&self.record).ok()?;
            post.labels
        }
    }

    impl<S: BosStr> Labeled<S> for ProfileView<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<S: BosStr> Labeled<S> for ProfileViewBasic<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<S: BosStr> Labeled<S> for ProfileViewDetailed<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for Post<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for Profile<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for Generator<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for List<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for Service<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }

    impl<S: BosStr> Labeled<S> for ListView<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<S: BosStr> Labeled<S> for Notification<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }
}

#[cfg(feature = "api_full")]
mod full_impls {
    //use super::*;
}

#[cfg(feature = "api_all")]
mod anisota_impls {
    use super::*;

    use jacquard_api::net_anisota::feed::{draft::Draft, post::Post};

    impl<S: BosStr + Clone> Labeled<S> for Post<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for Draft<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }
}

#[cfg(feature = "api_all")]
mod social_grain_impls {
    use super::*;
    use jacquard_api::social_grain::{
        actor::ProfileView,
        gallery::{Gallery, GalleryView},
    };

    impl<S: BosStr> Labeled<S> for ProfileView<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<S: BosStr> Labeled<S> for GalleryView<S> {
        fn labels(&self) -> &[Label<S>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<S: BosStr + Clone> Labeled<S> for Gallery<S> {
        fn labels(&self) -> &[Label<S>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<S>> {
            self.labels.clone()
        }
    }
}
