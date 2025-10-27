//! Implementation of #[lexicon_union] attribute macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Variant, parse2, punctuated::Punctuated, token::Comma};

/// Implementation for the lexicon_union attribute macro
pub fn impl_lexicon_union(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = match parse2::<DeriveInput>(item.clone()) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    match lexicon_union_impl(&input, item) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    }
}

fn lexicon_union_impl(input: &DeriveInput, original_item: TokenStream) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;

    // Extract refs from enum variants
    let refs = match &input.data {
        Data::Enum(enum_data) => extract_union_refs(&enum_data.variants)?,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "lexicon_union can only be applied to enums",
            ));
        }
    };

    if refs.is_empty() {
        return Err(syn::Error::new_spanned(
            input,
            "lexicon_union enum must have at least one variant with #[nsid = \"...\"] or #[serde(rename = \"...\")]",
        ));
    }

    // Generate the impl with LEXICON_UNION_REFS const
    Ok(quote! {
        #original_item

        impl #generics #name #generics {
            pub const LEXICON_UNION_REFS: &'static [&'static str] = &[
                #(#refs),*
            ];
        }
    })
}

fn extract_union_refs(variants: &Punctuated<Variant, Comma>) -> syn::Result<Vec<String>> {
    let mut refs = Vec::new();

    for variant in variants {
        let mut found_ref = None;

        // Priority 1: Check for #[nsid = "..."] attribute
        for attr in &variant.attrs {
            if attr.path().is_ident("nsid") {
                if let syn::Meta::NameValue(meta) = &attr.meta {
                    if let syn::Expr::Lit(expr_lit) = &meta.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            found_ref = Some(lit_str.value());
                            break;
                        }
                    }
                }
            }
        }

        // Priority 2: Check for #[serde(rename = "...")]
        if found_ref.is_none() {
            for attr in &variant.attrs {
                if !attr.path().is_ident("serde") {
                    continue;
                }

                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename") {
                        let value = meta.value()?;
                        let lit: syn::LitStr = value.parse()?;
                        found_ref = Some(lit.value());
                    }
                    Ok(())
                })?;

                if found_ref.is_some() {
                    break;
                }
            }
        }

        match found_ref {
            Some(ref_value) => refs.push(ref_value),
            None => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "lexicon_union enum variants must have #[nsid = \"...\"] or #[serde(rename = \"...\")] attribute",
                ));
            }
        }
    }

    Ok(refs)
}
