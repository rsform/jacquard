use heck::ToPascalCase;
use jacquard_common::CowStr;
use proc_macro2::TokenStream;
use quote::quote;

/// Convert a value string to a valid Rust variant name
pub(super) fn value_to_variant_name(value: &str) -> String {
    // Remove leading special chars and convert to pascal case
    let clean = value.trim_start_matches(|c: char| !c.is_alphanumeric());
    let variant = clean.replace('-', "_").to_pascal_case();

    // Prefix with underscore if starts with digit
    if variant.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        format!("_{}", variant)
    } else if variant.is_empty() {
        "Unknown".to_string()
    } else {
        variant
    }
}

/// Sanitize a string to be safe for identifiers and filenames
pub(super) fn sanitize_name(s: &str) -> String {
    if s.is_empty() {
        return "unknown".to_string();
    }

    // Replace invalid characters with underscores
    let mut sanitized: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Ensure it doesn't start with a digit
    if sanitized
        .chars()
        .next()
        .map_or(false, |c| c.is_ascii_digit())
    {
        sanitized = format!("_{}", sanitized);
    }

    sanitized
}

/// Create an identifier, using raw identifier if necessary for keywords
pub(super) fn make_ident(s: &str) -> syn::Ident {
    if s.is_empty() {
        eprintln!("Warning: Empty identifier encountered, using 'unknown' as fallback");
        return syn::Ident::new("unknown", proc_macro2::Span::call_site());
    }

    let sanitized = sanitize_name(s);

    // Try to parse as ident, fall back to raw ident if needed
    syn::parse_str::<syn::Ident>(&sanitized).unwrap_or_else(|_| {
        // only print if the sanitization actually changed the name
        // for types where the name is a keyword, will prepend 'r#'
        if s != sanitized {
            eprintln!(
                "Warning: Invalid identifier '{}' sanitized to '{}'",
                s, sanitized
            );
            syn::Ident::new(&sanitized, proc_macro2::Span::call_site())
        } else {
            syn::Ident::new_raw(&sanitized, proc_macro2::Span::call_site())
        }
    })
}

/// Generate doc comment from optional description
pub(super) fn generate_doc_comment(desc: Option<&CowStr>) -> TokenStream {
    if let Some(description) = desc {
        let desc_str = description.as_ref();
        quote! {
            #[doc = #desc_str]
        }
    } else {
        quote! {}
    }
}
