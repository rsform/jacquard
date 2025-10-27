//! Build method generation for builders
//!
//! Generates the final build() method that constructs the target type
//! once all required fields have been set.

use crate::codegen::builder_gen::BuilderSchema;
use crate::codegen::builder_gen::state_mod::RequiredField;
use crate::codegen::utils::make_ident;
use heck::ToSnakeCase;
use jacquard_common::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate the build() method for a builder
pub fn generate_build_method(
    type_name: &str,
    schema: &BuilderSchema,
    required_fields: &[RequiredField],
    has_lifetime: bool,
) -> TokenStream {
    let builder_name = format_ident!("{}Builder", type_name);
    let state_mod_name = format_ident!("{}_state", type_name.to_snake_case());
    let type_ident = format_ident!("{}", type_name);

    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    let lifetime_generic = if has_lifetime {
        quote! { <'a> }
    } else {
        quote! {}
    };

    // Generate where clauses for all required fields being IsSet
    let where_clauses = required_fields.iter().map(|field| {
        let field_pascal = format_ident!("{}", field.name_pascal.as_str());
        quote! { S::#field_pascal: #state_mod_name::IsSet, }
    });

    let required_set: std::collections::HashSet<&SmolStr> =
        required_fields.iter().map(|f| &f.name_snake).collect();

    // Generate field extractions for struct construction
    let property_names = schema.property_names();
    let field_extractions: Vec<_> = property_names
        .iter()
        .enumerate()
        .map(|(index, field_name)| {
            let field_snake = make_ident(&field_name.to_snake_case());
            let index = syn::Index::from(index);
            let is_required = required_set.contains(&SmolStr::new(field_name.to_snake_case()));

            if is_required {
                // Required field: unwrap from Option (guaranteed Some by IsSet constraint)
                quote! { #field_snake: self.__unsafe_private_named.#index.unwrap(), }
            } else {
                // Optional field: use directly
                quote! { #field_snake: self.__unsafe_private_named.#index, }
            }
        })
        .collect();

    // For LexObject (records/objects), add extra_data field and build_with_data method
    // LexXrpcParameters don't have extra_data
    let (extra_data_field, build_with_data_method) = match schema {
        BuilderSchema::Object(_) => {
            let default_field = quote! { extra_data: Default::default(), };

            let with_data_method = quote! {
                /// Build the final struct with custom extra_data
                pub fn build_with_data(
                    self,
                    extra_data: std::collections::BTreeMap<
                        jacquard_common::smol_str::SmolStr,
                        jacquard_common::types::value::Data #lifetime_generic
                    >,
                ) -> #type_ident #lifetime_generic {
                    #type_ident {
                        #(#field_extractions)*
                        extra_data: Some(extra_data),
                    }
                }
            };

            (default_field, with_data_method)
        }
        BuilderSchema::Parameters(_) => (quote! {}, quote! {}),
    };

    quote! {
        impl<#lifetime_param S> #builder_name<#lifetime_param S>
        where
            S: #state_mod_name::State,
            #(#where_clauses)*
        {
            /// Build the final struct
            pub fn build(self) -> #type_ident #lifetime_generic {
                #type_ident {
                    #(#field_extractions)*
                    #extra_data_field
                }
            }

            #build_with_data_method
        }
    }
}
