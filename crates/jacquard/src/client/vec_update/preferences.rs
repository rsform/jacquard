use jacquard_api::app_bsky::actor::PreferencesItem;
use jacquard_api::app_bsky::actor::get_preferences::{GetPreferences, GetPreferencesOutput};
use jacquard_api::app_bsky::actor::put_preferences::PutPreferences;

/// VecUpdate implementation for Bluesky actor preferences.
///
/// Provides get-modify-put operations on user preferences, which are stored
/// as a vec of preference items (each identified by enum discriminant).
///
/// # Example
///
/// ```ignore
/// use jacquard::client::vec_update::PreferencesUpdate;
/// use jacquard_api::app_bsky::actor::PreferencesItem;
///
/// // Update all preferences
/// agent.update_vec::<PreferencesUpdate>(|prefs| {
///     // Add a new preference
///     prefs.push(PreferencesItem::AdultContentPref(
///         Box::new(AdultContentPref { enabled: true })
///     ));
///
///     // Remove by variant
///     prefs.retain(|p| !matches!(p, PreferencesItem::InterestsPref(_)));
/// }).await?;
///
/// // Update a single preference (replaces by discriminant)
/// let pref = PreferencesItem::AdultContentPref(
///     Box::new(AdultContentPref { enabled: false })
/// );
/// agent.update_vec_item::<PreferencesUpdate>(pref).await?;
/// ```
pub struct PreferencesUpdate;

impl super::VecUpdate for PreferencesUpdate {
    type GetRequest = GetPreferences;
    type PutRequest = PutPreferences;
    type Item = PreferencesItem;

    fn build_get() -> Self::GetRequest {
        GetPreferences
    }

    fn extract_vec(output: GetPreferencesOutput) -> Vec<Self::Item> {
        output.preferences
    }

    fn build_put(items: Vec<Self::Item>) -> Self::PutRequest {
        PutPreferences::new().preferences(items).build()
    }

    fn matches(a: &Self::Item, b: &Self::Item) -> bool {
        // Match preferences by enum variant discriminant
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }
}
