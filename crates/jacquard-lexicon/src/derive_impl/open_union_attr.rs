//! Implementation of #[open_union] attribute macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Data, DeriveInput};

/// Implementation for the open_union attribute macro
pub fn impl_open_union(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = match parse2::<DeriveInput>(item) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    match &mut input.data {
        Data::Enum(data_enum) => {
            // Check if Unknown variant already exists
            let has_other = data_enum.variants.iter().any(|v| v.ident == "Unknown");

            if !has_other {
                // Determine the lifetime parameter to use
                let lifetime = if let Some(lt) = input.generics.lifetimes().next() {
                    quote! { #lt }
                } else {
                    quote! { 'static }
                };

                // Add the Unknown variant
                let new_variant: syn::Variant = syn::parse_quote! {
                    #[serde(untagged)]
                    Unknown(::jacquard_common::types::value::Data<#lifetime>)
                };
                data_enum.variants.push(new_variant);
            }

            quote! { #input }
        }
        _ => syn::Error::new_spanned(input, "open_union attribute can only be used on enums")
            .to_compile_error(),
    }
}
