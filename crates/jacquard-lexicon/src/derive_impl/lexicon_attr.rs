//! Implementation of `#[lexicon]` attribute macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse2};

/// Implementation for the lexicon attribute macro.
///
/// Detects whether the struct uses a type parameter `S` (BOS pattern) or a lifetime `'a`,
/// and emits the appropriate `extra_data` field.
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
                    // Check for a type parameter first (BOS pattern), then fall back to lifetime.
                    let has_type_param = input.generics.type_params().next().is_some();

                    let data_param = if has_type_param {
                        // Use the first type parameter (typically S).
                        let tp = input.generics.type_params().next().unwrap().ident.clone();
                        quote! { #tp }
                    } else if let Some(lt) = input.generics.lifetimes().next() {
                        quote! { #lt }
                    } else {
                        quote! { 'static }
                    };

                    // Only add serde(borrow) for lifetime-parameterised types, not type-parameterised.
                    let borrow_attr = if has_type_param {
                        quote! {}
                    } else {
                        quote! { #[serde(borrow)] }
                    };

                    let new_field: syn::Field = syn::parse_quote! {
                        #[serde(flatten)]
                        #borrow_attr
                        #[serde(skip_serializing_if = "Option::is_none")]
                        #[serde(default)]
                        pub extra_data: ::core::option::Option<::alloc::collections::BTreeMap<
                            ::jacquard_common::deps::smol_str::SmolStr,
                            ::jacquard_common::types::value::Data<#data_param>
                        >>
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
