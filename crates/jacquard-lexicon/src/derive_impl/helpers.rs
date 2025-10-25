//! Shared helper functions for derive macros

use syn::{Attribute, Ident};

/// Helper function to check if a struct derives bon::Builder or Builder
pub fn has_derive_builder(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }

        // Parse the derive attribute to check its contents
        if let Ok(list) = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
        ) {
            list.iter().any(|path| {
                // Check for "Builder" or "bon::Builder"
                path.segments
                    .last()
                    .map(|seg| seg.ident == "Builder")
                    .unwrap_or(false)
            })
        } else {
            false
        }
    })
}

/// Check if struct name conflicts with types referenced by bon::Builder macro.
/// bon::Builder generates code that uses unqualified `Option` and `Result`,
/// so structs with these names cause compilation errors.
pub fn conflicts_with_builder_macro(ident: &Ident) -> bool {
    matches!(ident.to_string().as_str(), "Option" | "Result")
}
