use jacquard_api::app_bsky::actor::PreferencesItem;
use jacquard_api::app_bsky::actor::get_preferences::{GetPreferences, GetPreferencesOutput};
use jacquard_api::app_bsky::actor::put_preferences::PutPreferences;
use jacquard_common::IntoStatic;

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
    type GetRequest<'de> = GetPreferences;
    type PutRequest<'de> = PutPreferences<'de>;
    type Item = PreferencesItem<'static>;

    fn build_get<'s>() -> Self::GetRequest<'s> {
        GetPreferences::new().build()
    }

    fn extract_vec<'s>(
        output: GetPreferencesOutput<'s>,
    ) -> Vec<<Self::Item as IntoStatic>::Output> {
        output
            .preferences
            .into_iter()
            .map(|p| p.into_static())
            .collect()
    }

    fn build_put<'s>(items: Vec<<Self::Item as IntoStatic>::Output>) -> Self::PutRequest<'s> {
        PutPreferences::new().preferences(items).build()
    }

    fn matches<'s>(a: &'s Self::Item, b: &'s Self::Item) -> bool {
        // Match preferences by enum variant discriminant
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }
}
