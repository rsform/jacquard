//! Implementation of #[derive(IntoStatic)] macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericParam, parse2};

/// Implementation for the IntoStatic derive macro.
///
/// Handles both lifetime-parameterised types (lifetimes → 'static) and
/// BOS type-parameterised types (S → S::Output with IntoStatic bound).
pub fn impl_derive_into_static(input: TokenStream) -> TokenStream {
    let input = match parse2::<DeriveInput>(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    let name = &input.ident;
    let generics = &input.generics;

    let has_type_params = generics.type_params().next().is_some();

    // Build the Output type generics:
    // - lifetimes → 'static
    // - type params → <T as IntoStatic>::Output
    let output_generics = generics.params.iter().map(|param| match param {
        GenericParam::Lifetime(_) => quote! { 'static },
        GenericParam::Type(ty) => {
            let ident = &ty.ident;
            quote! { <#ident as ::jacquard_common::IntoStatic>::Output }
        }
        GenericParam::Const(c) => {
            let ident = &c.ident;
            quote! { #ident }
        }
    });

    let output_type = if generics.params.is_empty() {
        quote! { #name }
    } else {
        quote! { #name<#(#output_generics),*> }
    };

    // For type-parameterised types, build custom impl bounds that add IntoStatic
    // and require Output to satisfy the same bounds as the original param.
    let (impl_block, where_block) = if has_type_params {
        // Build impl generics with IntoStatic added to type param bounds.
        let impl_params = generics.params.iter().map(|param| match param {
            GenericParam::Lifetime(lt) => quote! { #lt },
            GenericParam::Type(ty) => {
                let ident = &ty.ident;
                let existing_bounds = &ty.bounds;
                quote! { #ident: #existing_bounds + ::jacquard_common::IntoStatic }
            }
            GenericParam::Const(c) => quote! { #c },
        });

        // Build where clauses: T::Output must satisfy the same bounds as T.
        let output_bounds = generics.type_params().map(|ty| {
            let ident = &ty.ident;
            let bounds = &ty.bounds;
            quote! { <#ident as ::jacquard_common::IntoStatic>::Output: #bounds }
        });

        let existing_where = generics
            .where_clause
            .as_ref()
            .map(|w| {
                let predicates = &w.predicates;
                quote! { #predicates, }
            })
            .unwrap_or_default();

        (
            quote! { <#(#impl_params),*> },
            quote! { where #existing_where #(#output_bounds),* },
        )
    } else {
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        (
            quote! { #impl_generics },
            where_clause.map(|w| quote! { #w }).unwrap_or_default(),
        )
    };

    let (_, ty_generics, _) = generics.split_for_impl();

    // Generate the conversion body based on struct/enum
    let conversion = match &input.data {
        Data::Struct(data_struct) => generate_struct_conversion(name, &data_struct.fields),
        Data::Enum(data_enum) => generate_enum_conversion(name, data_enum),
        Data::Union(_) => {
            return syn::Error::new_spanned(input, "IntoStatic cannot be derived for unions")
                .to_compile_error();
        }
    };

    quote! {
        impl #impl_block ::jacquard_common::IntoStatic for #name #ty_generics #where_block {
            type Output = #output_type;

            fn into_static(self) -> Self::Output {
                #conversion
            }
        }
    }
}

fn generate_struct_conversion(name: &syn::Ident, fields: &Fields) -> TokenStream {
    match fields {
        Fields::Named(fields) => {
            let field_conversions = fields.named.iter().map(|f| {
                let field_name = &f.ident;
                quote! { #field_name: self.#field_name.into_static() }
            });
            quote! {
                #name {
                    #(#field_conversions),*
                }
            }
        }
        Fields::Unnamed(fields) => {
            let field_conversions = fields.unnamed.iter().enumerate().map(|(i, _)| {
                let index = syn::Index::from(i);
                quote! { self.#index.into_static() }
            });
            quote! {
                #name(#(#field_conversions),*)
            }
        }
        Fields::Unit => {
            quote! { #name }
        }
    }
}

fn generate_enum_conversion(name: &syn::Ident, data_enum: &syn::DataEnum) -> TokenStream {
    let variants = data_enum.variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                let field_conversions = field_names.iter().map(|field_name| {
                    quote! { #field_name: #field_name.into_static() }
                });
                quote! {
                    #name::#variant_name { #(#field_names),* } => {
                        #name::#variant_name {
                            #(#field_conversions),*
                        }
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_bindings: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| {
                        syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site())
                    })
                    .collect();
                let field_conversions = field_bindings.iter().map(|binding| {
                    quote! { #binding.into_static() }
                });
                quote! {
                    #name::#variant_name(#(#field_bindings),*) => {
                        #name::#variant_name(#(#field_conversions),*)
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #name::#variant_name => #name::#variant_name
                }
            }
        }
    });

    quote! {
        match self {
            #(#variants),*
        }
    }
}
