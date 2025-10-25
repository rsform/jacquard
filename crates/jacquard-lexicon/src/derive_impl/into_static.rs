//! Implementation of #[derive(IntoStatic)] macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Data, DeriveInput, Fields, GenericParam};

/// Implementation for the IntoStatic derive macro
pub fn impl_derive_into_static(input: TokenStream) -> TokenStream {
    let input = match parse2::<DeriveInput>(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    let name = &input.ident;
    let generics = &input.generics;

    // Build impl generics and where clause
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Build the Output type with all lifetimes replaced by 'static
    let output_generics = generics.params.iter().map(|param| match param {
        GenericParam::Lifetime(_) => quote! { 'static },
        GenericParam::Type(ty) => {
            let ident = &ty.ident;
            quote! { #ident }
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
        impl #impl_generics ::jacquard_common::IntoStatic for #name #ty_generics #where_clause {
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

fn generate_enum_conversion(
    name: &syn::Ident,
    data_enum: &syn::DataEnum,
) -> TokenStream {
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
