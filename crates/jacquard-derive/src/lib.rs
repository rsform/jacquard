use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

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
            // Check if Other variant already exists
            let has_other = data_enum.variants.iter().any(|v| v.ident == "Other");

            if !has_other {
                // Determine the lifetime parameter to use
                let lifetime = if let Some(lt) = input.generics.lifetimes().next() {
                    quote! { #lt }
                } else {
                    quote! { 'static }
                };

                // Add the Other variant
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
