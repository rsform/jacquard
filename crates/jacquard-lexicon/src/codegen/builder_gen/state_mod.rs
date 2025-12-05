//! State module generation for builders
//!
//! Generates the state trait, Empty state, and SetX<S> transition types
//! that enable type-safe builder patterns.

use std::collections::HashSet;

use heck::{ToPascalCase, ToSnakeCase};
use jacquard_common::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::codegen::utils::make_ident;

/// Information about a required field for builder state generation
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RequiredField {
    /// Field name (snake_case)
    pub name_snake: SmolStr,
    /// Field name (PascalCase)
    pub name_pascal: SmolStr,
}

impl RequiredField {
    pub fn new(field_name: &str) -> Self {
        Self {
            name_snake: SmolStr::new(field_name.to_snake_case()),
            name_pascal: SmolStr::new(field_name.to_pascal_case()),
        }
    }
}

/// Collect required fields from a builder schema
pub fn collect_required_fields(schema: &super::BuilderSchema<'_>) -> Vec<RequiredField> {
    let required = schema.required().unwrap_or(&[]);
    let nullable = schema.nullable().unwrap_or(&[]);

    let set: HashSet<_> = required
        .iter()
        .filter_map(|field_name| {
            if nullable.contains(&field_name) {
                None
            } else {
                let field_name_str: &str = field_name.as_ref();
                Some(RequiredField::new(field_name_str))
            }
        })
        .collect();

    set.into_iter().collect()
}

/// Generate the complete state module for a builder
pub fn generate_state_module(type_name: &str, required_fields: &[RequiredField]) -> TokenStream {
    let state_mod_name = format_ident!("{}_state", type_name.to_snake_case());

    let state_trait = generate_state_trait(required_fields);
    let empty_state = generate_empty_state(required_fields);
    let transition_types = generate_transition_types(required_fields);
    let members_mod = generate_members_module(required_fields);

    quote! {
        pub mod #state_mod_name {
            pub use crate::builder_types::{Set, Unset, IsSet, IsUnset};
            #[allow(unused)]
            use ::core::marker::PhantomData;

            mod sealed {
                pub trait Sealed {}
            }

            #state_trait
            #empty_state
            #transition_types
            #members_mod
        }
    }
}

/// Generate the State trait with associated types for each required field
fn generate_state_trait(required_fields: &[RequiredField]) -> TokenStream {
    let associated_types = required_fields.iter().map(|field| {
        let field_pascal = format_ident!("{}", field.name_pascal.as_str());
        quote! { type #field_pascal; }
    });

    quote! {
        /// State trait tracking which required fields have been set
        pub trait State: sealed::Sealed {
            #( #associated_types )*
        }
    }
}

/// Generate the Empty state (all fields Unset)
fn generate_empty_state(required_fields: &[RequiredField]) -> TokenStream {
    let field_impls = required_fields.iter().map(|field| {
        let field_pascal = format_ident!("{}", field.name_pascal.as_str());
        quote! {
            type #field_pascal = Unset;
        }
    });

    quote! {
        /// Empty state - all required fields are unset
        pub struct Empty(());

        impl sealed::Sealed for Empty {}

        impl State for Empty {
            #( #field_impls )*
        }
    }
}

/// Generate SetX<S> transition types for each required field
fn generate_transition_types(required_fields: &[RequiredField]) -> TokenStream {
    let transition_impls = required_fields.iter().map(|field| {
        let struct_name = format_ident!("Set{}", field.name_pascal.as_str());

        let doc = format!(
            "State transition - sets the `{}` field to Set",
            field.name_snake
        );

        // Generate associated type impls - this field is Set, others preserve S's state
        let field_impls = required_fields.iter().map(|other_field| {
            let other_pascal = format_ident!("{}", other_field.name_pascal.as_str());
            let other_snake = make_ident(other_field.name_snake.as_str());

            if field.name_snake == other_field.name_snake {
                // This field becomes Set
                quote! {
                    type #other_pascal = Set<members::#other_snake>;
                }
            } else {
                // Other fields preserve their state from S
                quote! {
                    type #other_pascal = S::#other_pascal;
                }
            }
        });

        quote! {
            #[doc = #doc]
            pub struct #struct_name<S: State = Empty>(PhantomData<fn() -> S>);

            impl<S: State> sealed::Sealed for #struct_name<S> {}

            impl<S: State> State for #struct_name<S> {
                #( #field_impls )*
            }
        }
    });

    quote! {
        #( #transition_impls )*
    }
}

/// Generate the members module with marker types for each field
fn generate_members_module(required_fields: &[RequiredField]) -> TokenStream {
    let member_types = required_fields.iter().map(|field| {
        let field_snake = make_ident(field.name_snake.as_str());
        let doc = format!("Marker type for the `{}` field", field.name_snake);

        quote! {
            #[doc = #doc]
            pub struct #field_snake(());
        }
    });

    quote! {
        /// Marker types for field names
        #[allow(non_camel_case_types)]
        pub mod members {
            #( #member_types )*
        }
    }
}
