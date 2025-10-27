//! Builder struct generation
//!
//! Generates the builder struct with State generic parameter and constructor methods.

use crate::codegen::builder_gen::BuilderSchema;
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate the complete builder struct including constructors
pub fn generate_builder_struct(
    codegen: &crate::codegen::CodeGenerator,
    nsid: &str,
    type_name: &str,
    schema: &BuilderSchema,
    has_lifetime: bool,
) -> TokenStream {
    let builder_name = format_ident!("{}Builder", type_name);
    let state_mod_name = format_ident!("{}_state", type_name.to_snake_case());
    let type_ident = format_ident!("{}", type_name);

    // Generate field declarations
    let field_decls = generate_field_declarations(codegen, nsid, type_name, schema);

    // Generate lifetime generic if needed
    let lifetime_generic = if has_lifetime {
        quote! { <'a> }
    } else {
        quote! {}
    };

    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    let phantom_field = if has_lifetime {
        quote! {
            _phantom: ::core::marker::PhantomData<&'a ()>,
        }
    } else {
        quote! {}
    };

    // Generate Struct::new() constructor on original type
    let struct_constructor = {
        quote! {
            impl #lifetime_generic #type_ident #lifetime_generic {
                /// Create a new builder for this type
                pub fn new() -> #builder_name<#lifetime_param #state_mod_name::Empty> {
                    #builder_name::new()
                }
            }
        }
    };

    // Generate Builder::new() constructor
    let builder_constructor =
        generate_builder_constructor(&builder_name, schema, has_lifetime, &state_mod_name);

    quote! {
        /// Builder for constructing an instance of this type
        pub struct #builder_name<#lifetime_param S: #state_mod_name::State> {
            #field_decls
            #phantom_field
        }

        #struct_constructor
        #builder_constructor
    }
}

/// Generate field declarations for the builder struct
/// All fields are stored in a single tuple of Options
fn generate_field_declarations(
    codegen: &crate::codegen::CodeGenerator,
    nsid: &str,
    type_name: &str,
    schema: &BuilderSchema,
) -> TokenStream {
    let property_names = schema.property_names();
    let field_types: Vec<_> = property_names
        .iter()
        .map(|field_name| {
            let field_name_str: &str = field_name.as_ref();
            let rust_type = match schema {
                BuilderSchema::Object(obj) => {
                    let field_type = &obj.properties[field_name_str];
                    codegen
                        .property_to_rust_type(nsid, type_name, field_name_str, field_type)
                        .unwrap_or_else(|_| quote! { () })
                }
                BuilderSchema::Parameters(params) => {
                    let field_type = &params.properties[field_name_str];
                    get_params_rust_type(codegen, field_type)
                }
            };

            quote! { ::core::option::Option<#rust_type>, }
        })
        .collect();

    if field_types.is_empty() {
        // No fields - empty tuple
        quote! {}
    } else {
        quote! {
            _phantom_state: ::core::marker::PhantomData<fn() -> S>,
            __unsafe_private_named: ( #(#field_types)* ),
        }
    }
}

/// Get Rust type for XRPC parameter property
pub(super) fn get_params_rust_type(
    codegen: &crate::codegen::CodeGenerator,
    field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
) -> TokenStream {
    use crate::lexicon::LexXrpcParametersProperty;

    match field_type {
        LexXrpcParametersProperty::Boolean(_) => quote! { bool },
        LexXrpcParametersProperty::Integer(_) => quote! { i64 },
        LexXrpcParametersProperty::String(s) => codegen.string_to_rust_type(s),
        LexXrpcParametersProperty::Unknown(_) => {
            quote! { jacquard_common::types::value::Data<'a> }
        }
        LexXrpcParametersProperty::Array(arr) => {
            let item_type = match &arr.items {
                crate::lexicon::LexPrimitiveArrayItem::Boolean(_) => quote! { bool },
                crate::lexicon::LexPrimitiveArrayItem::Integer(_) => quote! { i64 },
                crate::lexicon::LexPrimitiveArrayItem::String(s) => codegen.string_to_rust_type(s),
                crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => {
                    quote! { jacquard_common::types::value::Data<'a> }
                }
            };
            quote! { Vec<#item_type> }
        }
    }
}

/// Generate Builder::new() constructor with field initialization
fn generate_builder_constructor(
    builder_name: &syn::Ident,
    schema: &BuilderSchema,
    has_lifetime: bool,
    state_mod_name: &syn::Ident,
) -> TokenStream {
    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    // Initialize all fields as None in the tuple
    let property_names = schema.property_names();
    let none_values = property_names.iter().map(|_| quote! { None, });

    let (phantom_init, tuple_init) = if property_names.is_empty() {
        (quote! {}, quote! {})
    } else {
        (
            quote! {
                _phantom_state: ::core::marker::PhantomData,
            },
            quote! {
                __unsafe_private_named: ( #(#none_values)* ),
            },
        )
    };

    let phantom_lifetime = if has_lifetime {
        quote! {
            _phantom: ::core::marker::PhantomData,
        }
    } else {
        quote! {}
    };

    quote! {
        impl<#lifetime_param> #builder_name<#lifetime_param #state_mod_name::Empty> {
            /// Create a new builder with all fields unset
            pub fn new() -> Self {
                #builder_name {
                    #phantom_init
                    #tuple_init
                    #phantom_lifetime
                }
            }
        }
    }
}
