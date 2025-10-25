//! Implementation of #[lexicon] attribute macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse2};

use super::helpers::{conflicts_with_builder_macro, has_derive_builder};

/// Implementation for the lexicon attribute macro
pub fn impl_lexicon(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = match parse2::<DeriveInput>(item) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    match &mut input.data {
        Data::Struct(data_struct) => {
            if let Fields::Named(fields) = &mut data_struct.fields {
                // Check if extra_data field already exists
                let has_extra_data = fields
                    .named
                    .iter()
                    .any(|f| f.ident.as_ref().map(|i| i == "extra_data").unwrap_or(false));

                if !has_extra_data {
                    // Check if the struct derives bon::Builder and doesn't conflict with builder macro
                    let has_bon_builder = has_derive_builder(&input.attrs)
                        && !conflicts_with_builder_macro(&input.ident);

                    // Determine the lifetime parameter to use
                    let lifetime = if let Some(lt) = input.generics.lifetimes().next() {
                        quote! { #lt }
                    } else {
                        quote! { 'static }
                    };

                    // Add the extra_data field with serde(borrow) if there's a lifetime
                    let new_field: syn::Field = if has_bon_builder {
                        syn::parse_quote! {
                            #[serde(flatten)]
                            #[serde(borrow)]
                            #[serde(skip_serializing_if = "std::option::Option::is_none")]
                            #[serde(default)]
                            pub extra_data: ::std::option::Option<::std::collections::BTreeMap<
                                ::jacquard_common::smol_str::SmolStr,
                                ::jacquard_common::types::value::Data<#lifetime>
                            >>
                        }
                    } else {
                        syn::parse_quote! {
                            #[serde(flatten)]
                            #[serde(borrow)]
                            #[serde(skip_serializing_if = "std::option::Option::is_none")]
                            #[serde(default)]
                            pub extra_data: ::std::option::Option<::std::collections::BTreeMap<
                                ::jacquard_common::smol_str::SmolStr,
                                ::jacquard_common::types::value::Data<#lifetime>
                            >>
                        }
                    };
                    fields.named.push(new_field);
                }
            } else {
                return syn::Error::new_spanned(
                    input,
                    "lexicon attribute can only be used on structs with named fields",
                )
                .to_compile_error();
            }

            quote! { #input }
        }
        _ => syn::Error::new_spanned(input, "lexicon attribute can only be used on structs")
            .to_compile_error(),
    }
}
