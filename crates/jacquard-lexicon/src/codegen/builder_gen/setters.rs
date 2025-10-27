//! Setter generation for builder fields
//!
//! Generates typestate-constrained setters for required fields and
//! flexible setters for optional fields.

use crate::codegen::builder_gen::BuilderSchema;
use crate::codegen::builder_gen::state_mod::RequiredField;
use crate::codegen::utils::make_ident;
use heck::{ToPascalCase, ToSnakeCase};
use jacquard_common::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate all setters for a builder
pub fn generate_setters(
    codegen: &crate::codegen::CodeGenerator,
    nsid: &str,
    type_name: &str,
    schema: &BuilderSchema,
    required_fields: &[RequiredField],
    has_lifetime: bool,
) -> TokenStream {
    let builder_name = format_ident!("{}Builder", type_name);
    let state_mod_name = format_ident!("{}_state", type_name.to_snake_case());

    let required_set: std::collections::HashSet<&SmolStr> =
        required_fields.iter().map(|f| &f.name_snake).collect();

    let property_names = schema.property_names();
    let mut setters = Vec::new();

    for (index, field_name) in property_names.iter().enumerate() {
        let is_required = required_set.contains(&SmolStr::new(field_name.to_snake_case()));

        if is_required {
            // Generate required field setter with typestate transition
            let setter = generate_required_setter(
                &builder_name,
                &state_mod_name,
                codegen,
                nsid,
                type_name,
                field_name,
                schema,
                has_lifetime,
                index,
            );
            setters.push(setter);
        } else {
            // Generate optional field setters (.field() and .maybe_field())
            let setter = generate_optional_setter(
                &builder_name,
                &state_mod_name,
                codegen,
                nsid,
                type_name,
                field_name,
                schema,
                has_lifetime,
                index,
            );
            setters.push(setter);
        }
    }

    quote! {
        #(#setters)*
    }
}

/// Generate a setter for a required field with typestate transition
fn generate_required_setter(
    builder_name: &syn::Ident,
    state_mod_name: &syn::Ident,
    codegen: &crate::codegen::CodeGenerator,
    nsid: &str,
    type_name: &str,
    field_name: &SmolStr,
    schema: &BuilderSchema,
    has_lifetime: bool,
    index: usize,
) -> TokenStream {
    let field_snake = make_ident(&field_name.to_snake_case());
    let field_pascal = format_ident!("{}", field_name.to_pascal_case());
    let transition_type = format_ident!("Set{}", field_name.to_pascal_case());
    let index = syn::Index::from(index);

    // Get the Rust type for this field
    let rust_type = get_field_rust_type(codegen, nsid, type_name, field_name, schema);

    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    let phantom_lifetime = if has_lifetime {
        quote! { _phantom: ::core::marker::PhantomData, }
    } else {
        quote! {}
    };

    let doc = format!(" Set the `{}` field (required)", field_name);

    quote! {
        impl<#lifetime_param S> #builder_name<#lifetime_param S>
        where
            S: #state_mod_name::State,
            S::#field_pascal: #state_mod_name::IsUnset,
        {
            #[doc = #doc]
            pub fn #field_snake(
                mut self,
                value: impl Into<#rust_type>,
            ) -> #builder_name<#lifetime_param #state_mod_name::#transition_type<S>> {
                self.__unsafe_private_named.#index = ::core::option::Option::Some(value.into());
                #builder_name {
                    _phantom_state: ::core::marker::PhantomData,
                    __unsafe_private_named: self.__unsafe_private_named,
                    #phantom_lifetime
                }
            }
        }
    }
}

/// Generate setters for an optional field
fn generate_optional_setter(
    builder_name: &syn::Ident,
    state_mod_name: &syn::Ident,
    codegen: &crate::codegen::CodeGenerator,
    nsid: &str,
    type_name: &str,
    field_name: &SmolStr,
    schema: &BuilderSchema,
    has_lifetime: bool,
    index: usize,
) -> TokenStream {
    let field_snake = make_ident(&field_name.to_snake_case());
    let maybe_field_snake = format_ident!("maybe_{}", field_name.to_snake_case());
    let index = syn::Index::from(index);

    // Get the Rust type for this field
    let rust_type = get_field_rust_type(codegen, nsid, type_name, field_name, schema);

    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    let doc_field = format!(" Set the `{}` field (optional)", field_name);
    let doc_maybe = format!(
        " Set the `{}` field to an Option value (optional)",
        field_name
    );

    quote! {
        impl<#lifetime_param S: #state_mod_name::State> #builder_name<#lifetime_param S> {
            #[doc = #doc_field]
            pub fn #field_snake(mut self, value: impl Into<Option<#rust_type>>) -> Self {
                self.__unsafe_private_named.#index = value.into();
                self
            }

            #[doc = #doc_maybe]
            pub fn #maybe_field_snake(mut self, value: Option<#rust_type>) -> Self {
                self.__unsafe_private_named.#index = value;
                self
            }
        }
    }
}

/// Get the Rust type for a field
fn get_field_rust_type(
    codegen: &crate::codegen::CodeGenerator,
    nsid: &str,
    type_name: &str,
    field_name: &SmolStr,
    schema: &BuilderSchema,
) -> TokenStream {
    match schema {
        BuilderSchema::Object(obj) => {
            let field_type = &obj.properties[field_name];
            codegen
                .property_to_rust_type(nsid, type_name, field_name, field_type)
                .unwrap_or_else(|_| quote! { () })
        }
        BuilderSchema::Parameters(params) => {
            let field_type = &params.properties[field_name];
            super::builder_struct::get_params_rust_type(codegen, field_type)
        }
    }
}
