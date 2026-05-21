//! Build method generation for builders
//!
//! Generates the final build() method that constructs the target type
//! once all required fields have been set.

use crate::codegen::builder_gen::BuilderSchema;
use crate::codegen::builder_gen::state_mod::RequiredField;
use crate::codegen::utils::make_ident;
use crate::lexicon::LexObjectProperty;
use heck::ToSnakeCase;
use jacquard_common::deps::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate the build() method for a builder
pub fn generate_build_method(
    type_name: &str,
    schema: &BuilderSchema,
    required_fields: &[RequiredField],
    has_lifetime: bool,
    has_type_param: bool,
    resolved: &crate::codegen::prettify::ResolvedImports,
) -> TokenStream {
    let builder_name = format_ident!("{}Builder", type_name);
    let state_mod_name = format_ident!("{}_state", type_name.to_snake_case());
    let type_ident = format_ident!("{}", type_name);

    let bosstr_path =
        resolved.external_type_tokens(&crate::codegen::prettify::ExternalImport::BosStr);
    let lifetime_param = if has_lifetime {
        quote! { 'a, }
    } else {
        quote! {}
    };

    // S type parameter for the impl block (with bounds)
    let s_param = if has_type_param {
        quote! {, S: #bosstr_path }
    } else {
        quote! {}
    };

    // S arg for instantiating the builder or type (bare, no bounds)
    let s_arg = if has_type_param {
        quote! {, S }
    } else {
        quote! {}
    };

    // Return type generic for the built type.
    //
    // When has_type_param: the type carries `<S>` (not `<'a>`), so return `TypeIdent<S>`.
    // When !has_type_param but has_lifetime: return `TypeIdent<'a>`.
    // Otherwise: return plain `TypeIdent`.
    let type_return_generic = if has_type_param {
        quote! { <S> }
    } else if has_lifetime {
        quote! { <'a> }
    } else {
        quote! {}
    };

    // For build_with_data we need the Data type generic too.
    // Data always uses the same type parameter as the built type.
    let data_type_generic = if has_type_param {
        // Data<S> — the S flows through
        quote! { <S> }
    } else if has_lifetime {
        quote! { <'a> }
    } else {
        quote! {}
    };

    // Generate where clauses for all required fields being IsSet
    let where_clauses = required_fields.iter().map(|field| {
        let field_pascal = format_ident!("{}", field.name_pascal.as_str());
        quote! { St::#field_pascal: #state_mod_name::IsSet, }
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
                // Required field: unwrap from Option (guaranteed Some by IsSet constraint).
                quote! { #field_snake: self._fields.#index.unwrap(), }
            } else {
                // Optional field: apply schema default if present, otherwise pass through.
                let field_name_str: &str = field_name.as_ref();
                match schema
                    .get_object_property(field_name_str)
                    .and_then(|p| schema_default_expr(p, resolved))
                {
                    Some(default_val) => {
                        quote! {
                            #field_snake: self._fields.#index.or_else(|| Some(#default_val)),
                        }
                    }
                    None => {
                        quote! { #field_snake: self._fields.#index, }
                    }
                }
            }
        })
        .collect();

    // For LexObject (records/objects), add extra_data field and build_with_data method.
    // LexXrpcParameters don't have extra_data.
    let (extra_data_field, build_with_data_method) = match schema {
        BuilderSchema::Object(_) => {
            let default_field = quote! { extra_data: Default::default(), };

            let smol_str_path = resolved.type_path(&crate::codegen::prettify::CommonType::SmolStr);
            let data_path = resolved.type_path(&crate::codegen::prettify::CommonType::Data);
            let btree_path = resolved.btree_map_path();
            let with_data_method = quote! {
                /// Build the final struct with custom extra_data.
                pub fn build_with_data(
                    self,
                    extra_data: #btree_path<
                        #smol_str_path,
                        #data_path #data_type_generic
                    >,
                ) -> #type_ident #type_return_generic {
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
        impl<#lifetime_param St #s_param> #builder_name<#lifetime_param St #s_arg>
        where
            St: #state_mod_name::State,
            #(#where_clauses)*
        {
            /// Build the final struct.
            pub fn build(self) -> #type_ident #type_return_generic {
                #type_ident {
                    #(#field_extractions)*
                    #extra_data_field
                }
            }

            #build_with_data_method
        }
    }
}

/// Extract the default value expression for an object property, if one exists.
fn schema_default_expr(
    prop: &LexObjectProperty<'static>,
    _resolved: &crate::codegen::prettify::ResolvedImports,
) -> Option<TokenStream> {
    match prop {
        LexObjectProperty::Boolean(b) => {
            let v = b.default?;
            Some(quote! { #v })
        }
        LexObjectProperty::Integer(i) => {
            let v = i.default?;
            Some(quote! { #v })
        }
        LexObjectProperty::String(s) if s.known_values.is_none() => {
            let v = s.default.as_ref()?.as_ref();
            Some(quote! { S::from_static(#v) })
        }
        _ => None,
    }
}
