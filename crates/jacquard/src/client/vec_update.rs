/// Bluesky actor preferences implementation
pub mod preferences;

pub use preferences::PreferencesUpdate;

use jacquard_common::IntoStatic;
use jacquard_common::xrpc::{XrpcRequest, XrpcResp};

/// Trait for get-modify-put patterns on vec-based data structures.
///
/// This trait enables convenient update operations for endpoints that return arrays
/// that need to be fetched, modified, and put back. Common use cases include
/// preferences, saved feeds, and similar collection-style data.
///
/// # Example
///
/// ```ignore
/// use jacquard::client::vec_update::VecUpdate;
///
/// struct PreferencesUpdate;
///
/// impl VecUpdate for PreferencesUpdate {
///     type GetRequest = GetPreferences;
///     type PutRequest = PutPreferences;
///     type Item = PreferencesItem<'static>;
///
///     fn extract_vec(output: GetPreferencesOutput<'_>) -> Vec<Self::Item> {
///         output.preferences.into_iter().map(|p| p.into_static()).collect()
///     }
///
///     fn build_put(items: Vec<Self::Item>) -> PutPreferences {
///         PutPreferences { preferences: items }
///     }
///
///     fn matches(a: &Self::Item, b: &Self::Item) -> bool {
///         // Match by enum variant discriminant
///         std::mem::discriminant(a) == std::mem::discriminant(b)
///     }
/// }
/// ```
pub trait VecUpdate {
    /// The XRPC request type for fetching the data
    type GetRequest: XrpcRequest;

    /// The XRPC request type for putting the data back
    type PutRequest: XrpcRequest;

    /// The item type contained in the vec (must be owned/static)
    type Item: IntoStatic;

    /// Build the get request
    fn build_get() -> Self::GetRequest;

    /// Extract the vec from the get response output
    fn extract_vec<'s>(
        output: <<Self::GetRequest as XrpcRequest>::Response as XrpcResp>::Output<'s>,
    ) -> Vec<Self::Item>;

    /// Build the put request from the modified vec
    fn build_put(items: Vec<Self::Item>) -> Self::PutRequest;

    /// Check if two items match (for single-item update operations)
    ///
    /// This is used by `update_vec_item` to find and replace a single item in the vec.
    /// For example, preferences might match by enum variant discriminant.
    fn matches<'s>(a: &'s Self::Item, b: &'s Self::Item) -> bool;
}
