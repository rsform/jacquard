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
    has_type_param: bool,
    resolved: &crate::codegen::prettify::ResolvedImports,
) -> TokenStream {
    let builder_name = format_ident!("{}Builder", type_name);
    let state_mod_name = format_ident!("{}_state", type_name.to_snake_case());
    let type_ident = format_ident!("{}", type_name);

    // Generate field declarations
    let field_decls = generate_field_declarations(codegen, nsid, type_name, schema, resolved);

    // When has_type_param is true, the actual type (not the builder) carries S.
    // The builder still carries 'a for borrow-scoping via PhantomData.
    let bosstr_path =
        resolved.external_type_tokens(&crate::codegen::prettify::ExternalImport::BosStr);
    let default_str_path =
        resolved.external_type_tokens(&crate::codegen::prettify::ExternalImport::DefaultStr);

    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    // The S type parameter for the builder struct (when has_type_param)
    let builder_s_param = if has_type_param {
        quote! {, S: #bosstr_path = #default_str_path }
    } else {
        quote! {}
    };

    // The S arg for instantiating the builder (bare, no bounds)
    let builder_s_arg = if has_type_param {
        quote! {, S }
    } else {
        quote! {}
    };

    let phantom = resolved.phantom_data();
    let phantom_lifetime_field = if has_lifetime {
        quote! {
            _lifetime: #phantom<&'a ()>,
        }
    } else {
        quote! {}
    };
    let phantom_s_field = if has_type_param {
        quote! {
            _type: #phantom<fn() -> S>,
        }
    } else {
        quote! {}
    };

    // Generate Struct::new() constructor on original type.
    //
    // When has_type_param: the type is `Foo<S>` (no 'a lifetime), so the impl
    // is `impl<S: BosStr> Foo<S>`, and it returns a builder with
    // an elided lifetime for the borrow scope: `FooBuilder<'_, S, Empty>`.
    //
    // When !has_type_param: fall back to the old `impl<'a> Foo<'a>` pattern if
    // has_lifetime, or `impl Foo` if neither.
    let struct_constructor = if has_type_param {
        let type_s_impl = quote! { <S: #bosstr_path> };
        let lifetime = if has_lifetime {
            quote! { '_ }
        } else {
            quote! {}
        };
        quote! {
            impl #type_ident<DefaultStr> {
                /// Create a new builder for this type, using the default string type (DefaultStr = SmolStr) if needed
                pub fn new() -> #builder_name<#lifetime #state_mod_name::Empty, #default_str_path> {
                    #builder_name::new()
                }
            }

            impl #type_s_impl #type_ident<S> {
                /// Create a new builder for this type
                pub fn builder() -> #builder_name<#lifetime #state_mod_name::Empty #builder_s_arg> {
                    #builder_name::builder()
                }
            }
        }
    } else {
        let lifetime_generic = if has_lifetime {
            quote! { <'a> }
        } else {
            quote! {}
        };
        quote! {
            impl #lifetime_generic #type_ident #lifetime_generic {
                /// Create a new builder for this type.
                pub fn new() -> #builder_name<#lifetime_param #state_mod_name::Empty> {
                    #builder_name::new()
                }
            }
        }
    };

    // Generate Builder::new() constructor
    let builder_constructor = generate_builder_constructor(
        &builder_name,
        schema,
        has_lifetime,
        has_type_param,
        &state_mod_name,
        resolved,
    );

    quote! {
        /// Builder for constructing an instance of this type.
        pub struct #builder_name<#lifetime_param St: #state_mod_name::State #builder_s_param> {
            #field_decls
            #phantom_lifetime_field
            #phantom_s_field
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
    resolved: &crate::codegen::prettify::ResolvedImports,
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
                        .property_to_rust_type(
                            nsid,
                            type_name,
                            field_name_str,
                            field_type,
                            &resolved,
                        )
                        .unwrap_or_else(|_| quote! { () })
                }
                BuilderSchema::Parameters(params) => {
                    let field_type = &params.properties[field_name_str];
                    get_params_rust_type(codegen, field_type, &resolved)
                }
            };

            {
                let opt = resolved.option_type(rust_type);
                quote! { #opt, }
            }
        })
        .collect();

    let phantom = resolved.phantom_data();
    if field_types.is_empty() {
        // No fields - empty tuple
        quote! {}
    } else {
        quote! {
            _state: #phantom<fn() -> St>,
            _fields: ( #(#field_types)* ),
        }
    }
}

/// Get Rust type for XRPC parameter property
pub(super) fn get_params_rust_type(
    codegen: &crate::codegen::CodeGenerator,
    field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
    resolved: &crate::codegen::prettify::ResolvedImports,
) -> TokenStream {
    use crate::codegen::prettify::CommonType;
    use crate::lexicon::LexXrpcParametersProperty;

    match field_type {
        LexXrpcParametersProperty::Boolean(_) => quote! { bool },
        LexXrpcParametersProperty::Integer(_) => quote! { i64 },
        LexXrpcParametersProperty::String(s) => codegen.string_to_rust_type(s, resolved),
        LexXrpcParametersProperty::Unknown(_) => resolved.type_tokens(&CommonType::Data),
        LexXrpcParametersProperty::Array(arr) => {
            let item_type = match &arr.items {
                crate::lexicon::LexPrimitiveArrayItem::Boolean(_) => quote! { bool },
                crate::lexicon::LexPrimitiveArrayItem::Integer(_) => quote! { i64 },
                crate::lexicon::LexPrimitiveArrayItem::String(s) => {
                    codegen.string_to_rust_type(s, resolved)
                }
                crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => {
                    resolved.type_tokens(&CommonType::Data)
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
    has_type_param: bool,
    state_mod_name: &syn::Ident,
    resolved: &crate::codegen::prettify::ResolvedImports,
) -> TokenStream {
    let phantom = resolved.phantom_data();
    let bosstr_path =
        resolved.external_type_tokens(&crate::codegen::prettify::ExternalImport::BosStr);
    let default_str_path =
        resolved.external_type_tokens(&crate::codegen::prettify::ExternalImport::DefaultStr);

    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    // S type parameter for the impl block (with bounds)
    let s_param = if has_type_param && !has_lifetime {
        quote! {S: #bosstr_path}
    } else if has_type_param {
        quote! {, S: #bosstr_path}
    } else {
        quote! {}
    };

    // S arg for the builder instantiation path (bare, no bounds)
    let s_arg = if has_type_param {
        quote! {, S }
    } else {
        quote! {}
    };

    let default_str_param = if has_type_param {
        quote! {, #default_str_path }
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
                _state: #phantom,
            },
            quote! {
                _fields: ( #(#none_values)* ),
            },
        )
    };

    let phantom_lifetime_init = if has_lifetime {
        quote! {
            _lifetime: #phantom,
        }
    } else {
        quote! {}
    };

    let phantom_s_init = if has_type_param {
        quote! {
            _type: #phantom,
        }
    } else {
        quote! {}
    };

    quote! {
        impl<#lifetime_param> #builder_name<#lifetime_param #state_mod_name::Empty #default_str_param> {
            /// Create a new builder with all fields unset, using the default string type, if needed
            pub fn new() -> Self {
                #builder_name {
                    #phantom_init
                    #tuple_init
                    #phantom_lifetime_init
                    #phantom_s_init
                }
            }
        }

        impl<#lifetime_param #s_param> #builder_name<#lifetime_param #state_mod_name::Empty #s_arg> {
            /// Create a new builder with all fields unset
            pub fn builder() -> Self {
                #builder_name {
                    #phantom_init
                    #tuple_init
                    #phantom_lifetime_init
                    #phantom_s_init
                }
            }
        }
    }
}
