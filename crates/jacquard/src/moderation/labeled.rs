use jacquard_api::com_atproto::label::{Label, SelfLabels};

/// Trait for content that has labels attached
///
/// Implemented by types that can be moderated based on their labels.
/// This includes both labels from labeler services and self-labels applied by authors.
pub trait Labeled<'a> {
    /// Get the labels applied to this content by labeler services
    fn labels(&self) -> &[Label<'a>];

    /// Get self-labels applied by the content author
    fn self_labels(&'a self) -> Option<SelfLabels<'a>> {
        None
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

    impl<'a> Labeled<'a> for PostView<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }

        fn self_labels(&'a self) -> Option<SelfLabels<'a>> {
            let post = from_data::<Post<'a>>(&self.record).ok()?;
            post.labels
        }
    }

    impl<'a> Labeled<'a> for ProfileView<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<'a> Labeled<'a> for ProfileViewBasic<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<'a> Labeled<'a> for ProfileViewDetailed<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<'a> Labeled<'a> for Post<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }

    impl<'a> Labeled<'a> for Profile<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }

    impl<'a> Labeled<'a> for Generator<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&'a self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }

    impl<'a> Labeled<'a> for List<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&'a self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }

    impl<'a> Labeled<'a> for Service<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&'a self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }

    impl<'a> Labeled<'a> for ListView<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<'a> Labeled<'a> for Notification<'a> {
        fn labels(&self) -> &[Label<'a>] {
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

    impl<'a> Labeled<'a> for Post<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }

    impl<'a> Labeled<'a> for Draft<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<'a>> {
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

    impl<'a> Labeled<'a> for ProfileView<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<'a> Labeled<'a> for GalleryView<'a> {
        fn labels(&self) -> &[Label<'a>] {
            self.labels.as_deref().unwrap_or(&[])
        }
    }

    impl<'a> Labeled<'a> for Gallery<'a> {
        fn labels(&self) -> &[Label<'a>] {
            &[]
        }

        fn self_labels(&self) -> Option<SelfLabels<'a>> {
            self.labels.clone()
        }
    }
}
