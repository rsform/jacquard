//! Custom builder generation for lexicon types
//!
//! This module generates type-safe builders following bon's layered typestate pattern,
//! but at lexicon codegen time rather than via proc macros. This eliminates proc macro
//! overhead in the API crate while maintaining identical API surface.

use crate::lexicon::{LexObject, LexObjectProperty, LexXrpcParameters};
use proc_macro2::TokenStream;
use quote::quote;

/// Schema type that can have a builder generated
#[derive(Debug, Clone)]
pub(crate) enum BuilderSchema<'a> {
    /// XRPC query parameters or nested objects
    Object(&'a LexObject<'static>),
    /// XRPC query parameters
    Parameters(&'a LexXrpcParameters<'static>),
}

impl<'a> BuilderSchema<'a> {
    /// Get the required fields list
    pub fn required(&self) -> Option<&[jacquard_common::deps::smol_str::SmolStr]> {
        match self {
            BuilderSchema::Object(obj) => obj.required.as_deref(),
            BuilderSchema::Parameters(params) => params.required.as_deref(),
        }
    }

    pub fn nullable(&self) -> Option<&[jacquard_common::deps::smol_str::SmolStr]> {
        match self {
            BuilderSchema::Object(obj) => obj.nullable.as_deref(),
            BuilderSchema::Parameters(_) => None,
        }
    }

    /// Get the property names (field names)
    pub fn property_names(&self) -> Vec<&jacquard_common::deps::smol_str::SmolStr> {
        match self {
            BuilderSchema::Object(obj) => obj.properties.keys().collect(),
            BuilderSchema::Parameters(params) => params.properties.keys().collect(),
        }
    }

    /// Look up an object property by field name.
    /// Returns `None` for `Parameters` schemas (XRPC params handle defaults separately).
    pub fn get_object_property(&self, field_name: &str) -> Option<&LexObjectProperty<'static>> {
        match self {
            BuilderSchema::Object(obj) => obj.properties.get(field_name),
            BuilderSchema::Parameters(_) => None,
        }
    }
}

pub(crate) mod build_method;
pub(crate) mod builder_struct;
pub(crate) mod common;
pub(crate) mod setters;
pub(crate) mod state_mod;

#[cfg(test)]
mod tests;

/// Context for builder generation
pub(crate) struct BuilderGenContext<'a, 'c> {
    /// Code generator reference for type conversion
    pub codegen: &'a crate::codegen::CodeGenerator<'c>,
    /// NSID of the lexicon being processed
    pub nsid: &'a str,
    /// Type name for the struct
    pub type_name: &'a str,
    /// Schema to generate builder for
    pub schema: BuilderSchema<'a>,
    /// Whether the struct has a lifetime parameter
    pub has_lifetime: bool,
    /// Whether the struct has a type parameter S (BOS pattern)
    pub has_type_param: bool,
    /// Resolved imports for the current file.
    pub resolved: &'a crate::codegen::prettify::ResolvedImports,
}

impl<'a, 'c> BuilderGenContext<'a, 'c> {
    /// Create a new builder generation context from a LexObject
    pub fn from_object(
        codegen: &'a crate::codegen::CodeGenerator<'c>,
        nsid: &'a str,
        type_name: &'a str,
        object: &'a LexObject<'static>,
        has_type_param: bool,
        resolved: &'a crate::codegen::prettify::ResolvedImports,
    ) -> Self {
        Self {
            codegen,
            nsid,
            type_name,
            schema: BuilderSchema::Object(object),
            has_lifetime: false, // BOS types use S, not 'a
            has_type_param,
            resolved,
        }
    }

    /// Create a new builder generation context from LexXrpcParameters
    pub fn from_parameters(
        codegen: &'a crate::codegen::CodeGenerator<'c>,
        nsid: &'a str,
        type_name: &'a str,
        parameters: &'a LexXrpcParameters<'static>,
        has_type_param: bool,
        resolved: &'a crate::codegen::prettify::ResolvedImports,
    ) -> Self {
        Self {
            codegen,
            nsid,
            type_name,
            schema: BuilderSchema::Parameters(parameters),
            has_lifetime: false, // BOS types use S, not 'a
            has_type_param,
            resolved,
        }
    }

    /// Generate the complete builder for this type
    pub fn generate(&self) -> TokenStream {
        // if we got literally no params, don't bother.
        if self.schema.property_names().is_empty() {
            return quote! {};
        }
        // Collect required fields
        let required_fields = state_mod::collect_required_fields(&self.schema);

        // Generate state module
        let state_module = state_mod::generate_state_module(self.type_name, &required_fields);

        // Generate builder struct with constructors
        let builder_struct = builder_struct::generate_builder_struct(
            self.codegen,
            self.nsid,
            self.type_name,
            &self.schema,
            self.has_lifetime,
            self.has_type_param,
            self.resolved,
        );

        // Generate setters
        let setters = setters::generate_setters(
            self.codegen,
            self.nsid,
            self.type_name,
            &self.schema,
            &required_fields,
            self.has_lifetime,
            self.has_type_param,
            self.resolved,
        );

        // Generate build method
        let build_method = build_method::generate_build_method(
            self.type_name,
            &self.schema,
            &required_fields,
            self.has_lifetime,
            self.has_type_param,
            self.resolved,
        );

        quote! {
            #state_module
            #builder_struct
            #setters
            #build_method
        }
    }
}
