//! Attribute parsing functions

use super::types::*;
use syn::{Attribute, DeriveInput, LitStr};

/// Parse type-level lexicon attributes
pub fn parse_type_attrs(attrs: &[Attribute]) -> syn::Result<LexiconTypeAttrs> {
    let mut result = LexiconTypeAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("lexicon") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("nsid") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.nsid = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("fragment") {
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    let lit: LitStr = value.parse()?;
                    result.fragment = Some(lit.value());
                } else {
                    result.fragment = Some(String::new());
                }
                Ok(())
            } else if meta.path.is_ident("record") {
                result.kind = Some(LexiconTypeKind::Record);
                Ok(())
            } else if meta.path.is_ident("query") {
                result.kind = Some(LexiconTypeKind::Query);
                Ok(())
            } else if meta.path.is_ident("procedure") {
                result.kind = Some(LexiconTypeKind::Procedure);
                Ok(())
            } else if meta.path.is_ident("subscription") {
                result.kind = Some(LexiconTypeKind::Subscription);
                Ok(())
            } else if meta.path.is_ident("key") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.key = Some(lit.value());
                Ok(())
            } else {
                Err(meta.error("unknown lexicon attribute"))
            }
        })?;
    }

    Ok(result)
}

/// Parse field-level lexicon attributes
pub fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<LexiconFieldAttrs> {
    let mut result = LexiconFieldAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("lexicon") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("max_length") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.max_length = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("max_graphemes") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.max_graphemes = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("min_length") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.min_length = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("min_graphemes") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.min_graphemes = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("minimum") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.minimum = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("maximum") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.maximum = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("max_items") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.max_items = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("min_items") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.min_items = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("item_max_length") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.item_max_length = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("item_max_graphemes") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.item_max_graphemes = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("item_min_length") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.item_min_length = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("item_min_graphemes") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.item_min_graphemes = Some(lit.base10_parse()?);
                Ok(())
            } else if meta.path.is_ident("ref") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.explicit_ref = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("format") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.format = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("union") {
                // Check if there's a value (explicit type name)
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    let lit: LitStr = value.parse()?;
                    result.union_type = Some(Some(lit.value()));
                } else {
                    // Just #[lexicon(union)] - infer from field type
                    result.union_type = Some(None);
                }
                Ok(())
            } else {
                Err(meta.error("unknown lexicon field attribute"))
            }
        })?;
    }

    Ok(result)
}

/// Parse serde attributes for a field
pub fn parse_serde_attrs(attrs: &[Attribute]) -> syn::Result<SerdeAttrs> {
    let mut result = SerdeAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        // Use parse_args instead of parse_nested_meta to avoid consuming unknown attributes incorrectly
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                result.rename = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("skip") {
                result.skip = true;
                Ok(())
            } else {
                // Skip unknown attributes - but we need to consume the value if present
                if meta.input.peek(syn::Token![=]) {
                    let _ = meta.value()?;
                    let _ = meta.input.parse::<proc_macro2::TokenTree>();
                }
                Ok(())
            }
        });
    }

    Ok(result)
}

/// Parse container-level serde rename_all
pub fn parse_serde_rename_all(attrs: &[Attribute]) -> syn::Result<Option<RenameRule>> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        let mut found_rule = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                found_rule = RenameRule::from_str(&lit.value());
                Ok(())
            } else {
                Ok(())
            }
        })?;

        if found_rule.is_some() {
            return Ok(found_rule);
        }
    }

    // Default to camelCase (lexicon standard)
    Ok(Some(RenameRule::CamelCase))
}

/// Determine NSID from attributes and context
pub fn determine_nsid(attrs: &LexiconTypeAttrs, input: &DeriveInput) -> syn::Result<String> {
    if let Some(nsid) = &attrs.nsid {
        return Ok(nsid.clone());
    }

    if attrs.fragment.is_some() {
        return Err(syn::Error::new_spanned(
            input,
            "fragments require explicit nsid or module-level primary type (not yet implemented)",
        ));
    }

    // Check for XrpcRequest derive with NSID
    if let Some(nsid) = extract_xrpc_nsid(&input.attrs)? {
        return Ok(nsid);
    }

    Err(syn::Error::new_spanned(
        input,
        "missing required `nsid` attribute (use #[lexicon(nsid = \"...\")] or #[xrpc(nsid = \"...\")])",
    ))
}

/// Extract NSID from XrpcRequest attributes (cross-derive coordination)
fn extract_xrpc_nsid(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("xrpc") {
            continue;
        }

        let mut nsid = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("nsid") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                nsid = Some(lit.value());
            }
            Ok(())
        })?;

        if let Some(nsid) = nsid {
            return Ok(Some(nsid));
        }
    }
    Ok(None)
}

/// Extract T from Option<T>, return (type, is_required)
pub fn extract_option_inner(ty: &syn::Type) -> (&syn::Type, bool) {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (inner, false);
                    }
                }
            }
        }
    }
    (ty, true)
}

/// Check if type has #[open_union] attribute
pub fn has_open_union_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("open_union"))
}

/// Extract NSID ref for a variant
pub fn extract_variant_ref(variant: &syn::Variant, base_nsid: &str) -> syn::Result<String> {
    use heck::ToLowerCamelCase;

    // Priority 1: Check for #[nsid = "..."] attribute
    for attr in &variant.attrs {
        if attr.path().is_ident("nsid") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        return Ok(lit_str.value());
                    }
                }
            }
        }
    }

    // Priority 2: Check for #[serde(rename = "...")] attribute
    for attr in &variant.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        let mut rename = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit: LitStr = value.parse()?;
                rename = Some(lit.value());
            }
            Ok(())
        });

        if let Some(rename) = rename {
            return Ok(rename);
        }
    }

    // Priority 3: Generate fragment ref for unit variants
    match &variant.fields {
        syn::Fields::Unit => {
            let variant_name = variant.ident.to_string().to_lower_camel_case();
            Ok(format!("{}#{}", base_nsid, variant_name))
        }
        syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            Err(syn::Error::new_spanned(
                variant,
                "union variants with non-primitive types must use #[nsid] or #[serde(rename)] attribute to specify the ref",
            ))
        }
        _ => Err(syn::Error::new_spanned(
            variant,
            "union variants must be unit variants or have single unnamed field",
        )),
    }
}
