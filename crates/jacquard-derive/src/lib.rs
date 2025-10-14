//! # Derive macros for jacquard lexicon types
//!
//! This crate provides attribute and derive macros for working with Jacquard types.
//! The code generator uses `#[lexicon]` and `#[open_union]` to add lexicon-specific behavior.
//! You'll use `#[derive(IntoStatic)]` frequently, and `#[derive(XrpcRequest)]` when defining
//! custom XRPC endpoints.
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
//!
//! ### `#[derive(XrpcRequest)]`
//!
//! Derives XRPC request traits for custom endpoints. Generates the response marker struct
//! and implements `XrpcRequest` (and optionally `XrpcEndpoint` for server-side).
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, XrpcRequest)]
//! #[xrpc(
//!     nsid = "com.example.getThing",
//!     method = Query,
//!     output = GetThingOutput,
//! )]
//! struct GetThing<'a> {
//!     #[serde(borrow)]
//!     pub id: CowStr<'a>,
//! }
//! // Generates:
//! // - GetThingResponse struct
//! // - impl XrpcResp for GetThingResponse
//! // - impl XrpcRequest for GetThing
//! ```

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, GenericParam, Ident, LitStr, parse_macro_input};

/// Helper function to check if a struct derives bon::Builder or Builder
fn has_derive_builder(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }

        // Parse the derive attribute to check its contents
        if let Ok(list) = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
        ) {
            list.iter().any(|path| {
                // Check for "Builder" or "bon::Builder"
                path.segments
                    .last()
                    .map(|seg| seg.ident == "Builder")
                    .unwrap_or(false)
            })
        } else {
            false
        }
    })
}

/// Check if struct name conflicts with types referenced by bon::Builder macro.
/// bon::Builder generates code that uses unqualified `Option` and `Result`,
/// so structs with these names cause compilation errors.
fn conflicts_with_builder_macro(ident: &Ident) -> bool {
    matches!(ident.to_string().as_str(), "Option" | "Result")
}

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
                    let new_field: syn::Field = if input.generics.lifetimes().next().is_some() {
                        if has_bon_builder {
                            syn::parse_quote! {
                                #[serde(flatten)]
                                #[serde(borrow)]
                                #[builder(default)]
                                pub extra_data: ::std::collections::BTreeMap<
                                    ::jacquard_common::smol_str::SmolStr,
                                    ::jacquard_common::types::value::Data<#lifetime>
                                >
                            }
                        } else {
                            syn::parse_quote! {
                                #[serde(flatten)]
                                #[serde(borrow)]
                                pub extra_data: ::std::collections::BTreeMap<
                                    ::jacquard_common::smol_str::SmolStr,
                                    ::jacquard_common::types::value::Data<#lifetime>
                                >
                            }
                        }
                    } else {
                        // For types without lifetimes, make it optional to avoid lifetime conflicts
                        if has_bon_builder {
                            syn::parse_quote! {
                                #[serde(flatten)]
                                #[serde(skip_serializing_if = "std::option::Option::is_none")]
                                #[serde(default)]
                                #[builder(default)]
                                pub extra_data: Option<::std::collections::BTreeMap<
                                    ::jacquard_common::smol_str::SmolStr,
                                    ::jacquard_common::types::value::Data<'static>
                                >>
                            }
                        } else {
                            syn::parse_quote! {
                                #[serde(flatten)]
                                #[serde(skip_serializing_if = "std::option::Option::is_none")]
                                #[serde(default)]
                                pub extra_data:Option<::std::collections::BTreeMap<
                                    ::jacquard_common::smol_str::SmolStr,
                                    ::jacquard_common::types::value::Data<'static>
                                >>
                            }
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

/// Derive macro for `XrpcRequest` trait.
///
/// Automatically generates the response marker struct, `XrpcResp` impl, and `XrpcRequest` impl
/// for an XRPC endpoint. Optionally generates `XrpcEndpoint` impl for server-side usage.
///
/// # Attributes
///
/// - `nsid`: Required. The NSID string (e.g., "com.example.myMethod")
/// - `method`: Required. Either `Query` or `Procedure`
/// - `output`: Required. The output type (must support lifetime param if request does)
/// - `error`: Optional. Error type (defaults to `GenericError`)
/// - `server`: Optional flag. If present, generates `XrpcEndpoint` impl too
///
/// # Example
/// ```ignore
/// #[derive(Serialize, Deserialize, XrpcRequest)]
/// #[xrpc(
///     nsid = "com.example.getThing",
///     method = Query,
///     output = GetThingOutput,
/// )]
/// struct GetThing<'a> {
///     #[serde(borrow)]
///     pub id: CowStr<'a>,
/// }
/// ```
///
/// This generates:
/// - `GetThingResponse` struct implementing `XrpcResp`
/// - `XrpcRequest` impl for `GetThing`
/// - Optionally: `GetThingEndpoint` struct implementing `XrpcEndpoint` (if `server` flag present)
#[proc_macro_derive(XrpcRequest, attributes(xrpc))]
pub fn derive_xrpc_request(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match xrpc_request_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn xrpc_request_impl(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    // Parse attributes
    let attrs = parse_xrpc_attrs(&input.attrs)?;

    let name = &input.ident;
    let generics = &input.generics;

    // Detect if type has lifetime parameter
    let has_lifetime = generics.lifetimes().next().is_some();
    let lifetime = if has_lifetime {
        quote! { <'_> }
    } else {
        quote! {}
    };

    let nsid = &attrs.nsid;
    let method = method_expr(&attrs.method);
    let output_ty = &attrs.output;
    let error_ty = attrs
        .error
        .as_ref()
        .map(|e| quote! { #e })
        .unwrap_or_else(|| quote! { ::jacquard_common::xrpc::GenericError });

    // Generate response marker struct name
    let response_name = format_ident!("{}Response", name);

    // Build the impls
    let mut output = quote! {
        /// Response marker for #name
        pub struct #response_name;

        impl ::jacquard_common::xrpc::XrpcResp for #response_name {
            const NSID: &'static str = #nsid;
            const ENCODING: &'static str = "application/json";
            type Output<'de> = #output_ty<'de>;
            type Err<'de> = #error_ty<'de>;
        }

        impl #generics ::jacquard_common::xrpc::XrpcRequest for #name #lifetime {
            const NSID: &'static str = #nsid;
            const METHOD: ::jacquard_common::xrpc::XrpcMethod = #method;
            type Response = #response_name;
        }
    };

    // Optional server-side endpoint impl
    if attrs.server {
        let endpoint_name = format_ident!("{}Endpoint", name);
        let path = format!("/xrpc/{}", nsid);

        // Request type with or without lifetime
        let request_type = if has_lifetime {
            quote! { #name<'de> }
        } else {
            quote! { #name }
        };

        output.extend(quote! {
            /// Endpoint marker for #name (server-side)
            pub struct #endpoint_name;

            impl ::jacquard_common::xrpc::XrpcEndpoint for #endpoint_name {
                const PATH: &'static str = #path;
                const METHOD: ::jacquard_common::xrpc::XrpcMethod = #method;
                type Request<'de> = #request_type;
                type Response = #response_name;
            }
        });
    }

    Ok(output)
}

struct XrpcAttrs {
    nsid: String,
    method: XrpcMethod,
    output: syn::Type,
    error: Option<syn::Type>,
    server: bool,
}

enum XrpcMethod {
    Query,
    Procedure,
}

fn parse_xrpc_attrs(attrs: &[Attribute]) -> syn::Result<XrpcAttrs> {
    let mut nsid = None;
    let mut method = None;
    let mut output = None;
    let mut error = None;
    let mut server = false;

    for attr in attrs {
        if !attr.path().is_ident("xrpc") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("nsid") {
                let value = meta.value()?;
                let s: LitStr = value.parse()?;
                nsid = Some(s.value());
                Ok(())
            } else if meta.path.is_ident("method") {
                // Parse "method = Query" or "method = Procedure"
                let _eq = meta.input.parse::<syn::Token![=]>()?;
                let ident: Ident = meta.input.parse()?;
                match ident.to_string().as_str() {
                    "Query" => {
                        method = Some(XrpcMethod::Query);
                        Ok(())
                    }
                    "Procedure" => {
                        // Always JSON, no custom encoding support
                        method = Some(XrpcMethod::Procedure);
                        Ok(())
                    }
                    other => {
                        Err(meta
                            .error(format!("unknown method: {}, use Query or Procedure", other)))
                    }
                }
            } else if meta.path.is_ident("output") {
                let value = meta.value()?;
                output = Some(value.parse()?);
                Ok(())
            } else if meta.path.is_ident("error") {
                let value = meta.value()?;
                error = Some(value.parse()?);
                Ok(())
            } else if meta.path.is_ident("server") {
                server = true;
                Ok(())
            } else {
                Err(meta.error("unknown xrpc attribute"))
            }
        })?;
    }

    let nsid = nsid.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing required `nsid` attribute",
        )
    })?;
    let method = method.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing required `method` attribute",
        )
    })?;
    let output = output.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing required `output` attribute",
        )
    })?;

    Ok(XrpcAttrs {
        nsid,
        method,
        output,
        error,
        server,
    })
}

fn method_expr(method: &XrpcMethod) -> proc_macro2::TokenStream {
    match method {
        XrpcMethod::Query => quote! { ::jacquard_common::xrpc::XrpcMethod::Query },
        XrpcMethod::Procedure => {
            quote! { ::jacquard_common::xrpc::XrpcMethod::Procedure("application/json") }
        }
    }
}
