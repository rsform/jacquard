//! # Derive macros for jacquard lexicon types
//!
//! This crate provides attribute macros that the code generator uses to add lexicon-specific
//! behavior to generated types. You'll rarely need to use these directly unless you're writing
//! custom lexicon types by hand. However, deriving IntoStatic will likely be very useful.
//!
//! ## Macros
//!
//! ### `#[lexicon]`
//!
//! Adds an `extra_data` field to structs to capture unknown fields during deserialization.
//! This makes objects "open" - they'll accept and preserve fields not defined in the schema.
//!
//! ```ignore
//! #[lexicon]
//! struct Post<'s> {
//!     text: &'s str,
//! }
//! // Expands to add:
//! // #[serde(flatten)]
//! // pub extra_data: BTreeMap<SmolStr, Data<'s>>
//! ```
//!
//! ### `#[open_union]`
//!
//! Adds an `Unknown(Data)` variant to enums to make them extensible unions. This lets
//! enums accept variants not defined in your code, storing them as loosely typed atproto `Data`.
//!
//! ```ignore
//! #[open_union]
//! enum RecordEmbed<'s> {
//!     #[serde(rename = "app.bsky.embed.images")]
//!     Images(Images),
//! }
//! // Expands to add:
//! // #[serde(untagged)]
//! // Unknown(Data<'s>)
//! ```
//!
//! ### `#[derive(IntoStatic)]`
//!
//! Derives conversion from borrowed (`'a`) to owned (`'static`) types by recursively calling
//! `.into_static()` on all fields. Works with structs and enums.
//!
//! ```ignore
//! #[derive(IntoStatic)]
//! struct Post<'a> {
//!     text: CowStr<'a>,
//! }
//! // Generates:
//! // impl IntoStatic for Post<'_> {
//! //     type Output = Post<'static>;
//! //     fn into_static(self) -> Self::Output { ... }
//! // }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericParam, parse_macro_input};

/// Attribute macro that adds an `extra_data` field to structs to capture unknown fields
/// during deserialization.
///
/// # Example
/// ```ignore
/// #[lexicon]
/// struct Post<'s> {
///     text: &'s str,
/// }
/// // Expands to:
/// // struct Post<'s> {
/// //     text: &'s str,
/// //     #[serde(flatten)]
/// //     pub extra_data: BTreeMap<SmolStr, Data<'s>>,
/// // }
/// ```
#[proc_macro_attribute]
pub fn lexicon(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as DeriveInput);

    match &mut input.data {
        Data::Struct(data_struct) => {
            if let Fields::Named(fields) = &mut data_struct.fields {
                // Check if extra_data field already exists
                let has_extra_data = fields
                    .named
                    .iter()
                    .any(|f| f.ident.as_ref().map(|i| i == "extra_data").unwrap_or(false));

                if !has_extra_data {
                    // Determine the lifetime parameter to use
                    let lifetime = if let Some(lt) = input.generics.lifetimes().next() {
                        quote! { #lt }
                    } else {
                        quote! { 'static }
                    };

                    // Add the extra_data field with serde(borrow) if there's a lifetime
                    let new_field: syn::Field = if input.generics.lifetimes().next().is_some() {
                        syn::parse_quote! {
                            #[serde(flatten)]
                            #[serde(borrow)]
                            pub extra_data: ::std::collections::BTreeMap<
                                ::jacquard_common::smol_str::SmolStr,
                                ::jacquard_common::types::value::Data<#lifetime>
                            >
                        }
                    } else {
                        // For types without lifetimes, make it optional to avoid lifetime conflicts
                        syn::parse_quote! {
                            #[serde(flatten)]
                            #[serde(skip_serializing_if = "std::option::Option::is_none")]
                            #[serde(default)]
                            pub extra_data: std::option::Option<::std::collections::BTreeMap<
                                ::jacquard_common::smol_str::SmolStr,
                                ::jacquard_common::types::value::Data<'static>
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
                .to_compile_error()
                .into();
            }

            quote! { #input }.into()
        }
        _ => syn::Error::new_spanned(input, "lexicon attribute can only be used on structs")
            .to_compile_error()
            .into(),
    }
}

/// Attribute macro that adds an `Other(Data)` variant to enums to make them open unions.
///
/// # Example
/// ```ignore
/// #[open_union]
/// enum RecordEmbed<'s> {
///     #[serde(rename = "app.bsky.embed.images")]
///     Images(Images),
/// }
/// // Expands to:
/// // enum RecordEmbed<'s> {
/// //     #[serde(rename = "app.bsky.embed.images")]
/// //     Images(Images),
/// //     #[serde(untagged)]
/// //     Unknown(Data<'s>),
/// // }
/// ```
#[proc_macro_attribute]
pub fn open_union(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as DeriveInput);

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

            quote! { #input }.into()
        }
        _ => syn::Error::new_spanned(input, "open_union attribute can only be used on enums")
            .to_compile_error()
            .into(),
    }
}

/// Derive macro for `IntoStatic` trait.
///
/// Automatically implements conversion from borrowed to owned ('static) types.
/// Works with structs and enums that have lifetime parameters.
///
/// # Example
/// ```ignore
/// #[derive(IntoStatic)]
/// struct Post<'a> {
///     text: CowStr<'a>,
/// }
/// // Generates:
/// // impl IntoStatic for Post<'_> {
/// //     type Output = Post<'static>;
/// //     fn into_static(self) -> Self::Output {
/// //         Post { text: self.text.into_static() }
/// //     }
/// // }
/// ```
#[proc_macro_derive(IntoStatic)]
pub fn derive_into_static(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

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
                .to_compile_error()
                .into();
        }
    };

    let expanded = quote! {
        impl #impl_generics ::jacquard_common::IntoStatic for #name #ty_generics #where_clause {
            type Output = #output_type;

            fn into_static(self) -> Self::Output {
                #conversion
            }
        }
    };

    expanded.into()
}

fn generate_struct_conversion(name: &syn::Ident, fields: &Fields) -> proc_macro2::TokenStream {
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
) -> proc_macro2::TokenStream {
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
