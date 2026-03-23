use jacquard_api::com_atproto::label::{LabelValue, LabelValueDefinition};
use jacquard_common::bos::{BosStr, DefaultStr};
use jacquard_common::types::string::Did;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::hash::Hash;

/// User's moderation preferences
///
/// Specifies how the user wants to respond to different label values,
/// both globally and per-labeler.
///
/// HashMap keys are owned `SmolStr` values so that lookups remain efficient
/// without needing a generic string type parameter on this struct.
#[derive(Debug, Clone)]
pub struct ModerationPrefs {
    /// Whether adult content is enabled for this user
    pub adult_content_enabled: bool,
    /// Global label preferences (label value -> preference)
    pub labels: HashMap<SmolStr, LabelPref>,
    /// Per-labeler overrides (labeler DID -> label value -> preference)
    pub labelers: HashMap<Did, HashMap<SmolStr, LabelPref>>,
}

impl Default for ModerationPrefs {
    fn default() -> Self {
        Self {
            adult_content_enabled: false,
            labels: HashMap::new(),
            labelers: HashMap::new(),
        }
    }
}

/// User's preference for how to handle a specific label value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelPref {
    /// Hide the content completely
    Hide,
    /// Show with warning/blur
    Warn,
    /// Show normally (no filtering)
    Ignore,
}

/// Collection of labeler definitions
///
/// Maps labeler DIDs to their published label value definitions.
/// These definitions describe what labels mean, their severity, and default settings.
///
/// The type parameter `S` must satisfy `Hash + Eq` in addition to `BosStr` because
/// `Did<S>` is used as a `HashMap` key.
#[derive(Debug, Clone)]
pub struct LabelerDefs<S: BosStr + Hash + Eq = DefaultStr> {
    /// Labeler DID -> label value definitions
    pub defs: HashMap<Did<S>, Vec<LabelValueDefinition<S>>>,
}

/// Manual `Default` impl to avoid the derive macro adding an unnecessary `S: Default` bound.
impl<S: BosStr + Hash + Eq> Default for LabelerDefs<S> {
    fn default() -> Self {
        Self {
            defs: HashMap::new(),
        }
    }
}

impl<S: BosStr + Hash + Eq> LabelerDefs<S> {
    /// Create an empty set of labeler definitions
    pub fn new() -> Self {
        Self::default()
    }

    /// Add definitions for a labeler
    pub fn insert(&mut self, did: Did<S>, definitions: Vec<LabelValueDefinition<S>>) {
        self.defs.insert(did, definitions);
    }

    /// Get definitions for a specific labeler
    pub fn get(&self, did: &Did<impl BosStr>) -> Option<&[LabelValueDefinition<S>]> {
        self.defs
            .iter()
            .find(|(k, _)| k.as_ref() == did.as_ref())
            .map(|(_, v)| v.as_slice())
    }

    /// Find a label definition by labeler and identifier
    pub fn find_def(
        &self,
        labeler: &Did<impl BosStr>,
        identifier: &str,
    ) -> Option<&LabelValueDefinition<S>> {
        self.defs
            .iter()
            .find(|(k, _)| k.as_ref() == labeler.as_ref())
            .and_then(|(_, v)| v.iter().find(|def| def.identifier.as_ref() == identifier))
    }
}

/// Moderation decision for a piece of content
///
/// Describes what actions should be taken based on the labels applied to content
/// and the user's preferences.
#[derive(Debug, Clone, Default)]
pub struct ModerationDecision {
    /// Whether to hide the content completely
    pub filter: bool,
    /// What parts of the content to blur
    pub blur: Blur,
    /// Whether to show an alert-level warning
    pub alert: bool,
    /// Whether to show an informational badge
    pub inform: bool,
    /// Whether user override is allowed (false for legal takedowns)
    pub no_override: bool,
    /// Which labels caused this decision
    pub causes: Vec<LabelCause>,
}

impl ModerationDecision {
    /// Create a decision with no moderation applied
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether any moderation action is being taken
    pub fn is_moderated(&self) -> bool {
        self.filter || self.blur != Blur::None || self.alert || self.inform
    }
}

/// What parts of content should be blurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blur {
    /// No blurring
    #[default]
    None,
    /// Blur the entire content (text and media)
    Content,
    /// Blur media only (images, video, audio)
    Media,
}

/// Information about a label that contributed to a moderation decision
///
/// Uses `DefaultStr` (`SmolStr`) as the backing type, since causes are always
/// constructed at the point of moderation with owned values.
#[derive(Debug, Clone)]
pub struct LabelCause<S: BosStr = DefaultStr> {
    /// The label value that triggered this
    pub label: LabelValue<S>,
    /// Which labeler applied this label
    pub source: Did<S>,
    /// What the label is targeting
    pub target: LabelTarget,
}

/// What a label is targeting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelTarget {
    /// The label applies to an account/profile
    Account,
    /// The label applies to a specific piece of content
    Content,
}
