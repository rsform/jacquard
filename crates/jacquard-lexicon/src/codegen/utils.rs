use heck::ToPascalCase;
use jacquard_common::CowStr;
use proc_macro2::TokenStream;
use quote::quote;


/// Rust keywords that need escaping with r# prefix in module paths
const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "static", "struct",
    "super", "trait", "true", "type", "unsafe", "use", "where", "while",
    // Reserved keywords
    "abstract", "become", "box", "do", "final", "macro", "override", "priv",
    "try", "typeof", "unsized", "virtual", "yield",
    // 2018+ edition keywords
    "async", "await", "dyn",
];

/// Check if a string is a Rust keyword
#[inline]
fn is_rust_keyword(s: &str) -> bool {
    RUST_KEYWORDS.contains(&s)
}
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

/// Check if a string is already a valid identifier (alphanumeric + underscore, not starting with digit)
#[inline]
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    let first = chars.next().unwrap();

    // Must start with letter or underscore
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }

    // Rest must be alphanumeric or underscore
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Sanitize a string to be safe for identifiers and filenames, returning CowStr.
/// Borrows if already valid, allocates if modifications needed.
pub(super) fn sanitize_name_cow(s: &str) -> CowStr<'_> {
    if is_valid_identifier(s) {
        return CowStr::Borrowed(s);
    }

    if s.is_empty() {
        return CowStr::Owned(jacquard_common::smol_str::SmolStr::new_static("unknown"));
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

    CowStr::Owned(sanitized.into())
}

/// Sanitize a string to be safe for identifiers and filenames, always returning String.
/// Convenience wrapper around sanitize_name_cow for existing callsites.
pub(super) fn sanitize_name(s: &str) -> String {
    sanitize_name_cow(s).to_string()
}

/// Build namespace prefix from first two NSID segments (e.g., "com", "atproto" → "com_atproto")
pub(super) fn namespace_prefix(first: &str, second: &str) -> String {
    format!("{}_{}", sanitize_name_cow(first), sanitize_name_cow(second))
}

/// Escape a Rust keyword with r# prefix for use in paths
fn escape_keyword_for_path(s: &str) -> std::borrow::Cow<'_, str> {
    // crate, self, super, and Self are valid in path contexts
    if is_rust_keyword(s) && !matches!(s, "crate" | "self" | "super" | "Self") {
        std::borrow::Cow::Owned(format!("r#{}", s))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Join NSID segments into a module path (e.g., ["repo", "admin"] → "repo::admin")
pub(super) fn join_module_path(segments: &[&str]) -> String {
    segments
        .iter()
        .map(|s| {
            let sanitized = sanitize_name_cow(s);
            escape_keyword_for_path(&sanitized).into_owned()
        })
        .collect::<Vec<_>>()
        .join("::")
}

/// Join already-processed strings into a Rust module path (e.g., ["crate", "foo", "Bar"] → "crate::foo::Bar")
pub(super) fn join_path_parts(parts: &[impl AsRef<str>]) -> String {
    parts
        .iter()
        .map(|p| escape_keyword_for_path(p.as_ref()).into_owned())
        .collect::<Vec<_>>()
        .join("::")
}

/// Create an identifier, using raw identifier if necessary for keywords
pub fn make_ident(s: &str) -> syn::Ident {
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
        let desc_str = format!(" {description}");
        quote! {
            #[doc = #desc_str]
        }
    } else {
        quote! {}
    }
}
