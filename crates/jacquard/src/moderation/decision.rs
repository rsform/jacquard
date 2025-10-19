use super::{
    Blur, LabelCause, LabelPref, LabelTarget, Labeled, LabelerDefs, ModerationDecision,
    ModerationPrefs,
};
use jacquard_api::com_atproto::label::{Label, LabelValue};
use jacquard_common::IntoStatic;
use jacquard_common::types::string::{Datetime, Did};

/// Apply moderation logic to a single piece of content
///
/// Takes the content, user preferences, labeler definitions, and list of accepted labelers,
/// and produces a moderation decision indicating what actions to take.
///
/// # Arguments
///
/// * `item` - The content to moderate
/// * `prefs` - User's moderation preferences
/// * `defs` - Labeler definitions describing what labels mean
/// * `accepted_labelers` - Which labelers to trust (usually from CallOptions)
///
/// # Example
///
/// ```ignore
/// # use jacquard::moderation::*;
/// # use jacquard_api::app_bsky::feed::PostView;
/// # fn example(post: &PostView<'_>, prefs: &ModerationPrefs<'_>, defs: &LabelerDefs<'_>) {
/// let decision = moderate(post, prefs, defs, &[]);
/// if decision.filter {
///     println!("This post should be hidden");
/// }
/// # }
/// ```
pub fn moderate<'a, T: Labeled<'a>>(
    item: &'a T,
    prefs: &ModerationPrefs<'_>,
    defs: &LabelerDefs<'_>,
    accepted_labelers: &[Did<'_>],
) -> ModerationDecision {
    let mut decision = ModerationDecision::none();
    let now = Datetime::now();

    // Process labels from labeler services
    for label in item.labels() {
        // Skip expired labels
        if let Some(exp) = &label.exp {
            if exp <= &now {
                continue;
            }
        }

        // Skip labels from untrusted labelers (if acceptance list is provided)
        if !accepted_labelers.is_empty() && !accepted_labelers.contains(&label.src) {
            continue;
        }

        // Handle negation labels (remove previous causes)
        if label.neg.unwrap_or(false) {
            decision.causes.retain(|cause| {
                !(cause.label.as_str() == label.val.as_ref() && cause.source == label.src)
            });
            continue;
        }

        apply_label(label, prefs, defs, &mut decision);
    }

    // Process self-labels
    if let Some(self_labels) = item.self_labels() {
        for self_label in self_labels.values {
            // Self-labels don't have a source DID, so we'll use a placeholder approach
            // In practice, self-labels are usually just used for adult content marking

            // Check user preference for this label
            let pref = prefs
                .labels
                .iter()
                .find(|(k, _)| k.as_ref() == self_label.val.as_ref())
                .map(|(_, v)| v);

            // For self-labels, we generally respect them as warnings/info
            // unless user has explicitly set a preference
            match pref {
                Some(LabelPref::Hide) => {
                    decision.filter = true;
                }
                Some(LabelPref::Warn) | None => {
                    // Default to warning for self-labels
                    if decision.blur == Blur::None {
                        decision.blur = Blur::Content;
                    }
                    decision.inform = true;
                }
                Some(LabelPref::Ignore) => {
                    // User chose to ignore
                }
            }
        }
    }

    decision
}

/// Apply a single label to a moderation decision
fn apply_label(
    label: &Label<'_>,
    prefs: &ModerationPrefs<'_>,
    defs: &LabelerDefs<'_>,
    decision: &mut ModerationDecision,
) {
    let label_val = label.val.as_ref();

    // Get user preference (per-labeler override first, then global)
    let pref = prefs
        .labelers
        .get(&label.src)
        .and_then(|labeler_prefs| {
            labeler_prefs
                .iter()
                .find(|(k, _)| k.as_ref() == label_val)
                .map(|(_, v)| v)
        })
        .or_else(|| {
            prefs
                .labels
                .iter()
                .find(|(k, _)| k.as_ref() == label_val)
                .map(|(_, v)| v)
        });

    // Get label definition from the labeler
    let def = defs.find_def(&label.src, label_val);

    // Check if this is an adult-only label and adult content is disabled
    if let Some(def) = def {
        if def.adult_only.unwrap_or(false) && !prefs.adult_content_enabled {
            decision.filter = true;
            decision.no_override = true;
            decision.causes.push(LabelCause {
                label: LabelValue::from(label_val).into_static(),
                source: label.src.clone().into_static(),
                target: determine_target(label),
            });
            return;
        }
    }

    // Apply based on preference or default
    match pref.copied() {
        Some(LabelPref::Hide) => {
            decision.filter = true;
            decision.causes.push(LabelCause {
                label: LabelValue::from(label_val).into_static(),
                source: label.src.clone().into_static(),
                target: determine_target(label),
            });
        }
        Some(LabelPref::Warn) => {
            apply_warning(label, def, decision);
        }
        Some(LabelPref::Ignore) => {
            // User chose to ignore this label
        }
        None => {
            // No user preference - use default from definition or built-in defaults
            apply_default(label, def, decision);
        }
    }
}

/// Apply warning-level moderation based on label definition
fn apply_warning(
    label: &Label<'_>,
    def: Option<&jacquard_api::com_atproto::label::LabelValueDefinition<'_>>,
    decision: &mut ModerationDecision,
) {
    let label_val = label.val.as_ref();

    // Determine blur type from definition
    let blur = if let Some(def) = def {
        match def.blurs.as_ref() {
            "content" => Blur::Content,
            "media" => Blur::Media,
            _ => Blur::None,
        }
    } else {
        // Built-in defaults for known labels
        match label_val {
            "porn" | "sexual" | "nudity" | "nsfl" | "gore" => Blur::Media,
            _ => Blur::Content,
        }
    };

    // Apply blur (keep strongest blur if multiple labels)
    decision.blur = match (decision.blur, blur) {
        (Blur::Content, _) | (_, Blur::Content) => Blur::Content,
        (Blur::Media, _) | (_, Blur::Media) => Blur::Media,
        _ => Blur::None,
    };

    // Determine severity for alert vs inform
    if let Some(def) = def {
        match def.severity.as_ref() {
            "alert" => decision.alert = true,
            "inform" => decision.inform = true,
            _ => {}
        }
    } else {
        // Default to alert for warnings
        decision.alert = true;
    }

    decision.causes.push(LabelCause {
        label: LabelValue::from(label_val).into_static(),
        source: label.src.clone().into_static(),
        target: determine_target(label),
    });
}

/// Apply default moderation when user has no preference
fn apply_default(
    label: &Label<'_>,
    def: Option<&jacquard_api::com_atproto::label::LabelValueDefinition<'_>>,
    decision: &mut ModerationDecision,
) {
    let label_val = label.val.as_ref();

    // Check if definition has a default setting
    if let Some(def) = def {
        if let Some(default_setting) = &def.default_setting {
            match default_setting.as_ref() {
                "hide" => {
                    decision.filter = true;
                    decision.causes.push(LabelCause {
                        label: LabelValue::from(label_val).into_static(),
                        source: label.src.clone().into_static(),
                        target: determine_target(label),
                    });
                    return;
                }
                "warn" => {
                    apply_warning(label, Some(def), decision);
                    return;
                }
                "ignore" => return,
                _ => {}
            }
        }
    }

    // Built-in defaults for system labels (starting with !)
    if label_val.starts_with('!') {
        match label_val {
            "!hide" => {
                decision.filter = true;
                decision.no_override = true;
                decision.causes.push(LabelCause {
                    label: LabelValue::from(label_val).into_static(),
                    source: label.src.clone().into_static(),
                    target: determine_target(label),
                });
            }
            "!warn" => {
                apply_warning(label, def, decision);
            }
            "!no-unauthenticated" => {
                // This should be handled by auth layer, but we can note it
                decision.inform = true;
            }
            _ => {}
        }
    } else {
        // Built-in defaults for known content labels
        match label_val {
            "porn" | "nsfl" => {
                decision.filter = true;
                decision.causes.push(LabelCause {
                    label: LabelValue::from(label_val).into_static(),
                    source: label.src.clone().into_static(),
                    target: determine_target(label),
                });
            }
            "sexual" | "nudity" | "gore" => {
                apply_warning(label, def, decision);
            }
            _ => {
                // Unknown label - default to informational
                decision.inform = true;
                decision.causes.push(LabelCause {
                    label: LabelValue::from(label_val).into_static(),
                    source: label.src.clone().into_static(),
                    target: determine_target(label),
                });
            }
        }
    }
}

/// Determine whether a label targets an account or content
fn determine_target(label: &Label<'_>) -> LabelTarget {
    // Try to parse as a DID - this handles both:
    // - Bare DIDs: did:plc:xyz
    // - at:// URIs with only DID authority: at://did:plc:xyz
    // If it parses successfully, it's account-level.
    // If it fails, it must be a full URI with collection/rkey, so content-level.
    use jacquard_common::types::string::Did;

    if Did::new(label.uri.as_ref()).is_ok() {
        LabelTarget::Account
    } else {
        LabelTarget::Content
    }
}

/// Apply moderation to a slice of items
///
/// Returns a Vec of tuples containing the original item reference and its decision.
///
/// # Example
///
/// ```ignore
/// # use jacquard::moderation::*;
/// # use jacquard_api::app_bsky::feed::PostView;
/// # fn example(posts: &[PostView<'_>], prefs: &ModerationPrefs<'_>, defs: &LabelerDefs<'_>) {
/// let results = moderate_all(posts, prefs, defs, &[]);
/// for (post, decision) in results {
///     if decision.filter {
///         // skip this post
///     }
/// }
/// # }
/// ```
pub fn moderate_all<'a, T: Labeled<'a>>(
    items: &'a [T],
    prefs: &ModerationPrefs<'_>,
    defs: &LabelerDefs<'_>,
    accepted_labelers: &[Did<'_>],
) -> Vec<(&'a T, ModerationDecision)> {
    items
        .iter()
        .map(|item| (item, moderate(item, prefs, defs, accepted_labelers)))
        .collect()
}

/// Extension trait for applying moderation to iterators
///
/// Provides convenience methods for filtering and mapping moderation decisions
/// over collections.
pub trait ModerationIterExt<'a, T: Labeled<'a> + 'a>: Iterator<Item = &'a T> + Sized {
    /// Map each item to a tuple of (item, decision)
    fn with_moderation(
        self,
        prefs: &'a ModerationPrefs<'_>,
        defs: &'a LabelerDefs<'_>,
        accepted_labelers: &'a [Did<'_>],
    ) -> impl Iterator<Item = (&'a T, ModerationDecision)> {
        self.map(move |item| (item, moderate(item, prefs, defs, accepted_labelers)))
    }

    /// Filter out items that should be hidden
    fn filter_moderated(
        self,
        prefs: &'a ModerationPrefs<'_>,
        defs: &'a LabelerDefs<'_>,
        accepted_labelers: &'a [Did<'_>],
    ) -> impl Iterator<Item = &'a T> {
        self.filter(move |item| !moderate(*item, prefs, defs, accepted_labelers).filter)
    }
}

impl<'a, T: Labeled<'a> + 'a, I: Iterator<Item = &'a T>> ModerationIterExt<'a, T> for I {}
