//! Implementation of #[derive(XrpcRequest)] macro

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, Attribute, DeriveInput, Ident, LitStr};

/// Implementation for the XrpcRequest derive macro
pub fn impl_derive_xrpc_request(input: TokenStream) -> TokenStream {
    let input = match parse2::<DeriveInput>(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    match xrpc_request_impl(&input) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    }
}

fn xrpc_request_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
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

fn method_expr(method: &XrpcMethod) -> TokenStream {
    match method {
        XrpcMethod::Query => quote! { ::jacquard_common::xrpc::XrpcMethod::Query },
        XrpcMethod::Procedure => {
            quote! { ::jacquard_common::xrpc::XrpcMethod::Procedure("application/json") }
        }
    }
}
