//! Implementation of #[derive(LexiconSchema)] macro

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, parse2};

/// Implementation for the LexiconSchema derive macro
pub fn impl_derive_lexicon_schema(input: TokenStream) -> TokenStream {
    let input = match parse2::<DeriveInput>(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error(),
    };

    match lexicon_schema_impl(&input) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    }
}

fn lexicon_schema_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
    // Generate based on data type
    match &input.data {
        Data::Struct(_) => impl_for_struct(input),
        Data::Enum(_) => impl_for_enum(input),
        Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "LexiconSchema cannot be derived for unions",
        )),
    }
}

/// Struct implementation
fn impl_for_struct(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;

    // Detect lifetime
    let has_lifetime = generics.lifetimes().next().is_some();
    let lifetime = if has_lifetime {
        quote! { <'_> }
    } else {
        quote! {}
    };

    // Use schema builder to get actual data
    let built = crate::schema::from_ast::build_struct_schema(input)?;

    // Convert to tokens for code generation
    let doc_tokens = super::doc_to_tokens::doc_to_tokens(&built.doc);
    let validation_tokens = super::doc_to_tokens::validations_to_tokens(&built.validation_checks);

    let nsid = &built.nsid;
    let schema_id_expr = if built.schema_id != built.nsid {
        let sid = &built.schema_id;
        quote! { ::jacquard_common::CowStr::from(#sid) }
    } else {
        quote! { ::jacquard_common::CowStr::new_static(#nsid) }
    };

    // Generate trait impl
    Ok(quote! {
        impl #generics ::jacquard_lexicon::schema::LexiconSchema for #name #lifetime {
            fn nsid() -> &'static str {
                #nsid
            }

            fn schema_id() -> ::jacquard_common::CowStr<'static> {
                #schema_id_expr
            }

            fn lexicon_doc(
                _generator: &mut ::jacquard_lexicon::schema::LexiconGenerator
            ) -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                #doc_tokens
            }

            fn validate(&self) -> ::std::result::Result<(), ::jacquard_lexicon::schema::ValidationError> {
                #validation_tokens
            }
        }

        // Generate inventory submission for workspace discovery
        ::inventory::submit! {
            ::jacquard_lexicon::schema::LexiconSchemaRef {
                nsid: #nsid,
                provider: || {
                    let mut generator = ::jacquard_lexicon::schema::LexiconGenerator::new(#nsid);
                    <#name as ::jacquard_lexicon::schema::LexiconSchema>::lexicon_doc(&mut generator)
                },
            }
        }
    })
}

/// Enum implementation (union support)
fn impl_for_enum(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;

    // Detect lifetime
    let has_lifetime = generics.lifetimes().next().is_some();
    let lifetime = if has_lifetime {
        quote! { <'_> }
    } else {
        quote! {}
    };

    // Use schema builder to get actual data
    let built = crate::schema::from_ast::build_enum_schema(input)?;

    // Convert to tokens for code generation
    let doc_tokens = super::doc_to_tokens::doc_to_tokens(&built.doc);

    let nsid = &built.nsid;

    Ok(quote! {
        impl #generics ::jacquard_lexicon::schema::LexiconSchema for #name #lifetime {
            fn nsid() -> &'static str {
                #nsid
            }

            fn schema_id() -> ::jacquard_common::CowStr<'static> {
                ::jacquard_common::CowStr::new_static(#nsid)
            }

            fn lexicon_doc(
                _generator: &mut ::jacquard_lexicon::schema::LexiconGenerator
            ) -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                #doc_tokens
            }

            fn validate(&self) -> ::std::result::Result<(), ::jacquard_lexicon::schema::ValidationError> {
                Ok(())
            }
        }

        ::inventory::submit! {
            ::jacquard_lexicon::schema::LexiconSchemaRef {
                nsid: #nsid,
                provider: || {
                    let mut generator = ::jacquard_lexicon::schema::LexiconGenerator::new(#nsid);
                    <#name as ::jacquard_lexicon::schema::LexiconSchema>::lexicon_doc(&mut generator)
                },
            }
        }
    })
}
