use crate::error::Result;
use crate::lexicon::{
    LexArrayItem, LexObjectProperty, LexXrpcBody, LexXrpcBodySchema, LexXrpcError,
    LexXrpcProcedure, LexXrpcQuery, LexXrpcSubscription, LexXrpcSubscriptionMessageSchema,
};
use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::quote;

use super::CodeGenerator;
use super::utils::make_ident;

impl<'c> CodeGenerator<'c> {
    /// Generate query type
    pub(super) fn generate_query(
        &self,
        nsid: &str,
        def_name: &str,
        query: &LexXrpcQuery<'static>,
    ) -> Result<TokenStream> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut output = Vec::new();

        let params_has_lifetime = query
            .parameters
            .as_ref()
            .map(|p| match p {
                crate::lexicon::LexXrpcQueryParameter::Params(params) => {
                    self.params_need_lifetime(params)
                }
            })
            .unwrap_or(false);
        let has_params = query.parameters.is_some();
        let has_output = query.output.is_some();
        let has_errors = query.errors.is_some();

        if let Some(params) = &query.parameters {
            let params_struct = self.generate_params_struct(&type_base, params)?;
            output.push(params_struct);
        }

        if let Some(body) = &query.output {
            let output_struct = self.generate_output_struct(nsid, &type_base, body)?;
            output.push(output_struct);
        }

        if let Some(errors) = &query.errors {
            let error_enum = self.generate_error_enum(&type_base, errors)?;
            output.push(error_enum);
        }

        // Generate XrpcRequest impl
        let output_encoding = query
            .output
            .as_ref()
            .map(|o| o.encoding.as_ref())
            .unwrap_or("application/json");

        let xrpc_impl = self.generate_xrpc_request_impl(
            nsid,
            &type_base,
            quote! { jacquard_common::xrpc::XrpcMethod::Query },
            output_encoding,
            has_params,
            params_has_lifetime,
            has_output,
            has_errors,
            false, // queries never have binary inputs
        )?;
        output.push(xrpc_impl);

        Ok(quote! {
            #(#output)*
        })
    }

    /// Generate procedure type
    pub(super) fn generate_procedure(
        &self,
        nsid: &str,
        def_name: &str,
        proc: &LexXrpcProcedure<'static>,
    ) -> Result<TokenStream> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut output = Vec::new();

        // Check if input is a binary body (no schema)
        let is_binary_input = proc
            .input
            .as_ref()
            .map(|i| i.schema.is_none())
            .unwrap_or(false);

        // Input bodies with schemas have lifetimes (they get #[lexicon] attribute)
        // Binary inputs don't have lifetimes
        let params_has_lifetime = proc.input.is_some() && !is_binary_input;
        let has_input = proc.input.is_some();
        let has_output = proc.output.is_some();
        let has_errors = proc.errors.is_some();

        if let Some(params) = &proc.parameters {
            let params_struct = self.generate_params_struct_proc(&type_base, params)?;
            output.push(params_struct);
        }

        if let Some(body) = &proc.input {
            let input_struct = self.generate_input_struct(nsid, &type_base, body)?;
            output.push(input_struct);
        }

        if let Some(body) = &proc.output {
            let output_struct = self.generate_output_struct(nsid, &type_base, body)?;
            output.push(output_struct);
        }

        if let Some(errors) = &proc.errors {
            let error_enum = self.generate_error_enum(&type_base, errors)?;
            output.push(error_enum);
        }

        // Generate XrpcRequest impl
        let input_encoding = proc
            .input
            .as_ref()
            .map(|i| i.encoding.as_ref())
            .unwrap_or("application/json");
        let output_encoding = proc
            .output
            .as_ref()
            .map(|o| o.encoding.as_ref())
            .unwrap_or("application/json");
        let xrpc_impl = self.generate_xrpc_request_impl(
            nsid,
            &type_base,
            quote! { jacquard_common::xrpc::XrpcMethod::Procedure(#input_encoding) },
            output_encoding,
            has_input,
            params_has_lifetime,
            has_output,
            has_errors,
            is_binary_input,
        )?;
        output.push(xrpc_impl);

        Ok(quote! {
            #(#output)*
        })
    }

    pub(super) fn generate_subscription(
        &self,
        nsid: &str,
        def_name: &str,
        sub: &LexXrpcSubscription<'static>,
    ) -> Result<TokenStream> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut output = Vec::new();

        if let Some(params) = &sub.parameters {
            // Extract LexXrpcParameters from the enum
            match params {
                crate::lexicon::LexXrpcSubscriptionParameter::Params(params_inner) => {
                    let params_struct =
                        self.generate_params_struct_inner(&type_base, params_inner)?;
                    output.push(params_struct);
                }
            }
        }

        if let Some(message) = &sub.message {
            if let Some(schema) = &message.schema {
                let message_type = self.generate_subscription_message(nsid, &type_base, schema)?;
                output.push(message_type);
            }
        }

        if let Some(errors) = &sub.errors {
            let error_enum = self.generate_error_enum(&type_base, errors)?;
            output.push(error_enum);
        }

        // Generate XrpcSubscription trait impl
        let params_has_lifetime = sub
            .parameters
            .as_ref()
            .map(|p| match p {
                crate::lexicon::LexXrpcSubscriptionParameter::Params(params) => {
                    self.params_need_lifetime(params)
                }
            })
            .unwrap_or(false);

        let has_params = sub.parameters.is_some();
        let has_message = sub.message.is_some();
        let has_errors = sub.errors.is_some();

        let subscription_impl = self.generate_xrpc_subscription_impl(
            nsid,
            &type_base,
            has_params,
            params_has_lifetime,
            has_message,
            has_errors,
        )?;
        output.push(subscription_impl);

        Ok(quote! {
            #(#output)*
        })
    }

    pub(super) fn generate_subscription_message(
        &self,
        nsid: &str,
        type_base: &str,
        schema: &LexXrpcSubscriptionMessageSchema<'static>,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcSubscriptionMessageSchema;

        match schema {
            LexXrpcSubscriptionMessageSchema::Union(union) => {
                // Generate a union enum for the message
                let enum_name = format!("{}Message", type_base);
                let enum_ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

                let mut variants = Vec::new();
                let mut decode_arms = Vec::new();

                for ref_str in &union.refs {
                    let ref_str_s = ref_str.as_ref();

                    // Normalize local refs (starting with #) by prepending current NSID
                    let normalized_ref = if ref_str.starts_with('#') {
                        format!("{}{}", nsid, ref_str)
                    } else {
                        ref_str.to_string()
                    };

                    // Parse ref to get NSID and def name
                    let (ref_nsid, ref_def) =
                        if let Some((nsid_part, fragment)) = normalized_ref.split_once('#') {
                            (nsid_part, fragment)
                        } else {
                            (normalized_ref.as_str(), "main")
                        };

                    let variant_name = if ref_def == "main" {
                        ref_nsid.split('.').last().unwrap().to_pascal_case()
                    } else {
                        ref_def.to_pascal_case()
                    };
                    let variant_ident =
                        syn::Ident::new(&variant_name, proc_macro2::Span::call_site());
                    let type_path = self.ref_to_rust_type(&normalized_ref)?;

                    variants.push(quote! {
                        #[serde(rename = #ref_str_s)]
                        #variant_ident(Box<#type_path>)
                    });

                    // Generate decode arm for framed decoding
                    decode_arms.push(quote! {
                        #ref_str_s => {
                            let variant = serde_ipld_dagcbor::from_slice(body)?;
                            Ok(Self::#variant_ident(Box::new(variant)))
                        }
                    });
                }

                let doc = self.generate_doc_comment(union.description.as_ref());

                // Generate decode_framed method for DAG-CBOR subscriptions
                let decode_framed_impl = quote! {
                    impl<'a> #enum_ident<'a> {
                        /// Decode a framed DAG-CBOR message (header + body).
                        pub fn decode_framed<'de: 'a>(bytes: &'de [u8]) -> Result<#enum_ident<'a>, jacquard_common::error::DecodeError> {
                            let (header, body) = jacquard_common::xrpc::subscription::parse_event_header(bytes)?;
                            match header.t.as_str() {
                                #(#decode_arms)*
                                unknown => Err(jacquard_common::error::DecodeError::UnknownEventType(
                                    unknown.into()
                                )),
                            }
                        }
                    }
                };

                Ok(quote! {
                    #doc
                    #[jacquard_derive::open_union]
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                    #[serde(tag = "$type")]
                    #[serde(bound(deserialize = "'de: 'a"))]
                    pub enum #enum_ident<'a> {
                        #(#variants,)*
                    }

                    #decode_framed_impl
                })
            }
            LexXrpcSubscriptionMessageSchema::Object(obj) => {
                // Generate a struct for the message
                let struct_name = format!("{}Message", type_base);
                let struct_ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

                let fields = self.generate_object_fields("", &struct_name, obj, false)?;
                let doc = self.generate_doc_comment(obj.description.as_ref());

                // Subscription message structs always get a lifetime since they have the #[lexicon] attribute
                // which adds extra_data: BTreeMap<..., Data<'a>>
                let struct_def = quote! {
                    #doc
                    #[jacquard_derive::lexicon]
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                    #[serde(rename_all = "camelCase")]
                    pub struct #struct_ident<'a> {
                        #fields
                    }
                };

                // Generate union types for this message
                let mut unions = Vec::new();
                for (field_name, field_type) in &obj.properties {
                    match field_type {
                        LexObjectProperty::Union(union) => {
                            // Skip empty, single-variant unions unless they're self-referential
                            if !union.refs.is_empty()
                                && (union.refs.len() > 1
                                    || self.is_self_referential_union(nsid, &struct_name, union))
                            {
                                let union_name = self.generate_field_type_name(
                                    nsid,
                                    &struct_name,
                                    field_name,
                                    "",
                                );
                                let refs: Vec<_> = union.refs.iter().cloned().collect();
                                let union_def = self.generate_union(
                                    nsid,
                                    &union_name,
                                    &refs,
                                    None,
                                    union.closed,
                                )?;
                                unions.push(union_def);
                            }
                        }
                        LexObjectProperty::Array(array) => {
                            if let LexArrayItem::Union(union) = &array.items {
                                // Skip single-variant array unions
                                if union.refs.len() > 1 {
                                    let union_name = self.generate_field_type_name(
                                        nsid,
                                        &struct_name,
                                        field_name,
                                        "Item",
                                    );
                                    let refs: Vec<_> = union.refs.iter().cloned().collect();
                                    let union_def = self.generate_union(
                                        nsid,
                                        &union_name,
                                        &refs,
                                        None,
                                        union.closed,
                                    )?;
                                    unions.push(union_def);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                Ok(quote! {
                    #struct_def
                    #(#unions)*
                })
            }
            LexXrpcSubscriptionMessageSchema::Ref(ref_type) => {
                // Just a type alias to the referenced type
                // Refs generally have lifetimes, so always add <'a>
                let type_name = format!("{}Message", type_base);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let rust_type = self.ref_to_rust_type(&ref_type.r#ref)?;
                let doc = self.generate_doc_comment(ref_type.description.as_ref());

                Ok(quote! {
                    #doc
                    pub type #ident<'a> = #rust_type;
                })
            }
        }
    }

    /// Generate params struct from XRPC query parameters
    pub(super) fn generate_params_struct(
        &self,
        type_base: &str,
        params: &crate::lexicon::LexXrpcQueryParameter<'static>,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcQueryParameter;
        match params {
            LexXrpcQueryParameter::Params(p) => self.generate_params_struct_inner(type_base, p),
        }
    }

    /// Generate params struct from XRPC procedure parameters (query string params)
    pub(super) fn generate_params_struct_proc(
        &self,
        type_base: &str,
        params: &crate::lexicon::LexXrpcProcedureParameter<'static>,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcProcedureParameter;
        match params {
            // For procedures, query string params still get "Params" suffix since the main struct is the input
            LexXrpcProcedureParameter::Params(p) => {
                let struct_name = format!("{}Params", type_base);
                let ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());
                self.generate_params_struct_inner_with_name(&ident, p)
            }
        }
    }

    /// Generate params struct inner (shared implementation)
    pub(super) fn generate_params_struct_inner(
        &self,
        type_base: &str,
        p: &crate::lexicon::LexXrpcParameters<'static>,
    ) -> Result<TokenStream> {
        let ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());
        self.generate_params_struct_inner_with_name(&ident, p)
    }

    /// Generate params struct with custom name
    pub(super) fn generate_params_struct_inner_with_name(
        &self,
        ident: &syn::Ident,
        p: &crate::lexicon::LexXrpcParameters<'static>,
    ) -> Result<TokenStream> {
        let required = p.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
        let mut fields = Vec::new();
        let mut default_fns = Vec::new();

        for (field_name, field_type) in &p.properties {
            let is_required = required.contains(field_name);
            let (field_tokens, default_fn) =
                self.generate_param_field_with_default("", field_name, field_type, is_required)?;
            fields.push(field_tokens);
            if let Some(fn_def) = default_fn {
                default_fns.push(fn_def);
            }
        }

        let doc = self.generate_doc_comment(p.description.as_ref());
        let needs_lifetime = self.params_need_lifetime(p);

        let derives = quote! {
            #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, bon::Builder, jacquard_derive::IntoStatic)]
            #[builder(start_fn = new)]
        };

        if needs_lifetime {
            Ok(quote! {
                #(#default_fns)*

                #doc
                #derives
                #[serde(rename_all = "camelCase")]
                pub struct #ident<'a> {
                    #(#fields)*
                }
            })
        } else {
            Ok(quote! {
                #(#default_fns)*

                #doc
                #derives
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #(#fields)*
                }
            })
        }
    }

    /// Generate input struct from XRPC body
    pub(super) fn generate_input_struct(
        &self,
        nsid: &str,
        type_base: &str,
        body: &LexXrpcBody<'static>,
    ) -> Result<TokenStream> {
        let ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

        // Check if this is a binary body (no schema, just raw bytes)
        let is_binary_body = body.schema.is_none();

        // Determine if we should derive Default or bon::Builder
        // Binary bodies always get builder, schema-based inputs use heuristics
        let (has_default, has_builder) = if is_binary_body {
            (false, true)
        } else if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
            use crate::codegen::structs::{
                all_required_are_defaultable_strings, conflicts_with_builder_macro,
                count_required_fields,
            };
            let required_count = count_required_fields(obj);
            let can_default = required_count == 0 || all_required_are_defaultable_strings(obj);
            let can_builder =
                required_count >= 1 && !can_default && !conflicts_with_builder_macro(type_base);
            (can_default, can_builder)
        } else {
            (false, false)
        };

        let fields = if let Some(schema) = &body.schema {
            self.generate_body_fields("", type_base, schema, has_builder)?
        } else {
            // Binary body: just a bytes field
            quote! {
                pub body: bytes::Bytes,
            }
        };

        let doc = self.generate_doc_comment(body.description.as_ref());

        // Binary bodies don't need #[lexicon] attribute or lifetime
        let struct_def = if is_binary_body {
            quote! {
                #doc
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, bon::Builder, jacquard_derive::IntoStatic)]
                #[builder(start_fn = new)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #fields
                }
            }
        } else if has_builder {
            // Input structs with schemas and builders: manually add extra_data field with #[builder(default)]
            // for bon compatibility. The #[lexicon] macro will see it exists and skip adding it.
            quote! {
                #doc
                #[jacquard_derive::lexicon]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, bon::Builder, jacquard_derive::IntoStatic)]
                #[serde(rename_all = "camelCase")]
                #[builder(start_fn = new)]
                pub struct #ident<'a> {
                    #fields
                    #[serde(flatten)]
                    #[serde(borrow)]
                    #[builder(default)]
                    pub extra_data: ::std::collections::BTreeMap<
                        ::jacquard_common::smol_str::SmolStr,
                        ::jacquard_common::types::value::Data<'a>
                    >,
                }
            }
        } else if has_default {
            quote! {
                #doc
                #[jacquard_derive::lexicon]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic, Default)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident<'a> {
                    #fields
                }
            }
        } else {
            quote! {
                #doc
                #[jacquard_derive::lexicon]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident<'a> {
                    #fields
                }
            }
        };

        // Generate union types if schema is an Object
        let mut unions = Vec::new();
        if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
            for (field_name, field_type) in &obj.properties {
                match field_type {
                    LexObjectProperty::Union(union) => {
                        // Skip empty, single-variant unions unless they're self-referential
                        if !union.refs.is_empty()
                            && (union.refs.len() > 1
                                || self.is_self_referential_union(nsid, type_base, union))
                        {
                            let union_name =
                                self.generate_field_type_name(nsid, type_base, field_name, "");
                            let refs: Vec<_> = union.refs.iter().cloned().collect();
                            let union_def =
                                self.generate_union(nsid, &union_name, &refs, None, union.closed)?;
                            unions.push(union_def);
                        }
                    }
                    LexObjectProperty::Array(array) => {
                        if let LexArrayItem::Union(union) = &array.items {
                            // Skip single-variant array unions
                            if union.refs.len() > 1 {
                                let union_name = self
                                    .generate_field_type_name(nsid, type_base, field_name, "Item");
                                let refs: Vec<_> = union.refs.iter().cloned().collect();
                                let union_def = self.generate_union(
                                    nsid,
                                    &union_name,
                                    &refs,
                                    None,
                                    union.closed,
                                )?;
                                unions.push(union_def);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(quote! {
            #struct_def
            #(#unions)*
        })
    }

    /// Generate output struct from XRPC body
    pub(super) fn generate_output_struct(
        &self,
        nsid: &str,
        type_base: &str,
        body: &LexXrpcBody<'static>,
    ) -> Result<TokenStream> {
        let struct_name = format!("{}Output", type_base);
        let ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

        let fields = if let Some(schema) = &body.schema {
            self.generate_body_fields("", &struct_name, schema, false)?
        } else {
            quote! {
                pub body: bytes::Bytes,
            }
        };

        let doc = self.generate_doc_comment(body.description.as_ref());

        // Determine if we should derive Default
        // Check if schema is an Object and apply heuristics
        let has_default = if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema
        {
            use crate::codegen::structs::{
                all_required_are_defaultable_strings, count_required_fields,
            };
            let required_count = count_required_fields(obj);
            required_count == 0 || all_required_are_defaultable_strings(obj)
        } else {
            false
        };

        // Output structs always get a lifetime since they have the #[lexicon] attribute
        // which adds extra_data: BTreeMap<..., Data<'a>>
        let struct_def = if has_default {
            quote! {
                #doc
                #[jacquard_derive::lexicon]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic, Default)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident<'a> {
                    #fields
                }
            }
        } else if body.schema.is_none() {
            quote! {
                #doc
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #fields
                }
            }
        } else {
            quote! {
                #doc
                #[jacquard_derive::lexicon]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(rename_all = "camelCase")]
                pub struct #ident<'a> {
                    #fields
                }
            }
        };

        // Generate union types if schema is an Object
        let mut unions = Vec::new();
        if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
            for (field_name, field_type) in &obj.properties {
                match field_type {
                    LexObjectProperty::Union(union) => {
                        // Skip single-variant unions unless they're self-referential
                        if union.refs.len() > 1
                            || self.is_self_referential_union(nsid, &struct_name, union)
                        {
                            let union_name =
                                self.generate_field_type_name(nsid, &struct_name, field_name, "");
                            let refs: Vec<_> = union.refs.iter().cloned().collect();
                            let union_def =
                                self.generate_union(nsid, &union_name, &refs, None, union.closed)?;
                            unions.push(union_def);
                        }
                    }
                    LexObjectProperty::Array(array) => {
                        if let LexArrayItem::Union(union) = &array.items {
                            // Skip single-variant array unions
                            if union.refs.len() > 1 {
                                let union_name = self.generate_field_type_name(
                                    nsid,
                                    &struct_name,
                                    field_name,
                                    "Item",
                                );
                                let refs: Vec<_> = union.refs.iter().cloned().collect();
                                let union_def = self.generate_union(
                                    nsid,
                                    &union_name,
                                    &refs,
                                    None,
                                    union.closed,
                                )?;
                                unions.push(union_def);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(quote! {
            #struct_def
            #(#unions)*
        })
    }

    /// Generate fields from XRPC body schema
    pub(super) fn generate_body_fields(
        &self,
        nsid: &str,
        parent_type_name: &str,
        schema: &LexXrpcBodySchema<'static>,
        is_builder: bool,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcBodySchema;

        match schema {
            LexXrpcBodySchema::Object(obj) => {
                self.generate_object_fields(nsid, parent_type_name, obj, is_builder)
            }
            LexXrpcBodySchema::Ref(ref_type) => {
                let rust_type = self.ref_to_rust_type(&ref_type.r#ref)?;
                Ok(quote! {
                    #[serde(flatten)]
                    #[serde(borrow)]
                    pub value: #rust_type,
                })
            }
            LexXrpcBodySchema::Union(_union) => {
                let rust_type = quote! { jacquard_common::types::value::Data<'a> };
                Ok(quote! {
                    #[serde(flatten)]
                    #[serde(borrow)]
                    pub value: #rust_type,
                })
            }
        }
    }

    /// Generate a field for XRPC parameters
    pub(super) fn generate_param_field(
        &self,
        _nsid: &str,
        field_name: &str,
        field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
        is_required: bool,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcParametersProperty;

        let field_ident = make_ident(&field_name.to_snake_case());

        let (rust_type, needs_lifetime, is_cowstr) = match field_type {
            LexXrpcParametersProperty::Boolean(_) => (quote! { bool }, false, false),
            LexXrpcParametersProperty::Integer(_) => (quote! { i64 }, false, false),
            LexXrpcParametersProperty::String(s) => {
                let is_cowstr = s.format.is_none(); // CowStr for plain strings
                (
                    self.string_to_rust_type(s),
                    self.string_needs_lifetime(s),
                    is_cowstr,
                )
            }
            LexXrpcParametersProperty::Unknown(_) => (
                quote! { jacquard_common::types::value::Data<'a> },
                true,
                false,
            ),
            LexXrpcParametersProperty::Array(arr) => {
                let needs_lifetime = match &arr.items {
                    crate::lexicon::LexPrimitiveArrayItem::Boolean(_)
                    | crate::lexicon::LexPrimitiveArrayItem::Integer(_) => false,
                    crate::lexicon::LexPrimitiveArrayItem::String(s) => {
                        self.string_needs_lifetime(s)
                    }
                    crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => true,
                };
                let item_type = match &arr.items {
                    crate::lexicon::LexPrimitiveArrayItem::Boolean(_) => quote! { bool },
                    crate::lexicon::LexPrimitiveArrayItem::Integer(_) => quote! { i64 },
                    crate::lexicon::LexPrimitiveArrayItem::String(s) => self.string_to_rust_type(s),
                    crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => {
                        quote! { jacquard_common::types::value::Data<'a> }
                    }
                };
                (quote! { Vec<#item_type> }, needs_lifetime, false)
            }
        };

        let rust_type = if is_required {
            rust_type
        } else {
            quote! { std::option::Option<#rust_type> }
        };

        let mut attrs = Vec::new();

        if !is_required {
            attrs.push(quote! { #[serde(skip_serializing_if = "std::option::Option::is_none")] });
        }

        // Add serde(borrow) to all fields with lifetimes
        if needs_lifetime {
            attrs.push(quote! { #[serde(borrow)] });
        }

        // Add builder(into) for CowStr fields to allow String, &str, etc.
        if is_cowstr {
            attrs.push(quote! { #[builder(into)] });
        }

        Ok(quote! {
            #(#attrs)*
            pub #field_ident: #rust_type,
        })
    }

    /// Generate param field with serde default if present
    /// Returns (field_tokens, optional_default_function)
    pub(super) fn generate_param_field_with_default(
        &self,
        nsid: &str,
        field_name: &str,
        field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
        is_required: bool,
    ) -> Result<(TokenStream, Option<TokenStream>)> {
        use crate::lexicon::LexXrpcParametersProperty;
        use heck::ToSnakeCase;

        // Get base field
        let base_field = self.generate_param_field(nsid, field_name, field_type, is_required)?;

        // Generate default function and attribute for required fields with defaults
        // For optional fields, just add doc comments
        let (doc_comment, serde_attr, default_fn) = if is_required {
            match field_type {
                LexXrpcParametersProperty::Boolean(b) if b.default.is_some() => {
                    let v = b.default.unwrap();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!("Defaults to `{}`", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident() -> bool { #v }
                        }),
                    )
                }
                LexXrpcParametersProperty::Integer(i) if i.default.is_some() => {
                    let v = i.default.unwrap();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!("Defaults to `{}`", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident() -> i64 { #v }
                        }),
                    )
                }
                LexXrpcParametersProperty::String(s) if s.default.is_some() => {
                    let v = s.default.as_ref().unwrap().as_ref();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!("Defaults to `\"{}\"`", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident() -> jacquard_common::CowStr<'static> {
                                jacquard_common::CowStr::from(#v)
                            }
                        }),
                    )
                }
                _ => (None, None, None),
            }
        } else {
            // Optional fields - just doc comments, no serde defaults
            let doc = match field_type {
                LexXrpcParametersProperty::Integer(i) => {
                    let mut parts = Vec::new();
                    if let Some(def) = i.default {
                        parts.push(format!("default: {}", def));
                    }
                    if let Some(min) = i.minimum {
                        parts.push(format!("min: {}", min));
                    }
                    if let Some(max) = i.maximum {
                        parts.push(format!("max: {}", max));
                    }
                    if !parts.is_empty() {
                        Some(format!("({})", parts.join(", ")))
                    } else {
                        None
                    }
                }
                LexXrpcParametersProperty::String(s) => {
                    let mut parts = Vec::new();
                    if let Some(def) = s.default.as_ref() {
                        parts.push(format!("default: \"{}\"", def.as_ref()));
                    }
                    if let Some(min) = s.min_length {
                        parts.push(format!("min length: {}", min));
                    }
                    if let Some(max) = s.max_length {
                        parts.push(format!("max length: {}", max));
                    }
                    if !parts.is_empty() {
                        Some(format!("({})", parts.join(", ")))
                    } else {
                        None
                    }
                }
                LexXrpcParametersProperty::Boolean(b) => {
                    b.default.map(|v| format!("(default: {})", v))
                }
                _ => None,
            };
            (doc, None, None)
        };

        let doc = doc_comment.as_ref().map(|d| quote! { #[doc = #d] });
        let field_with_attrs = match (doc, serde_attr) {
            (Some(doc), Some(attr)) => quote! {
                #doc
                #attr
                #base_field
            },
            (Some(doc), None) => quote! {
                #doc
                #base_field
            },
            (None, Some(attr)) => quote! {
                #attr
                #base_field
            },
            (None, None) => base_field,
        };

        Ok((field_with_attrs, default_fn))
    }

    /// Generate error enum from XRPC errors
    pub(super) fn generate_error_enum(
        &self,
        type_base: &str,
        errors: &[LexXrpcError<'static>],
    ) -> Result<TokenStream> {
        let enum_name = format!("{}Error", type_base);
        let ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

        let mut variants = Vec::new();
        let mut display_arms = Vec::new();

        for error in errors {
            let variant_name = error.name.to_pascal_case();
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            let error_name = error.name.as_ref();
            let doc = self.generate_doc_comment(error.description.as_ref());

            variants.push(quote! {
                #doc
                #[serde(rename = #error_name)]
                #variant_ident(std::option::Option<String>)
            });

            display_arms.push(quote! {
                Self::#variant_ident(msg) => {
                    write!(f, #error_name)?;
                    if let Some(msg) = msg {
                        write!(f, ": {}", msg)?;
                    }
                    Ok(())
                }
            });
        }

        // IntoStatic impl is generated by the derive macro now

        Ok(quote! {
            #[jacquard_derive::open_union]
            #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic, jacquard_derive::IntoStatic)]
            #[serde(tag = "error", content = "message")]
            #[serde(bound(deserialize = "'de: 'a"))]
            pub enum #ident<'a> {
                #(#variants,)*
            }

            impl std::fmt::Display for #ident<'_> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        #(#display_arms)*
                        Self::Unknown(err) => write!(f, "Unknown error: {:?}", err),
                    }
                }
            }
        })
    }

    /// Generate XrpcRequest trait impl for a query or procedure
    pub(super) fn generate_xrpc_request_impl(
        &self,
        nsid: &str,
        type_base: &str,
        method: TokenStream,
        output_encoding: &str,
        has_params: bool,
        params_has_lifetime: bool,
        has_output: bool,
        has_errors: bool,
        is_binary_input: bool,
    ) -> Result<TokenStream> {
        let output_type = if has_output {
            let output_ident = syn::Ident::new(
                &format!("{}Output", type_base),
                proc_macro2::Span::call_site(),
            );
            if output_encoding == "application/json" {
                quote! {
                    #output_ident<'de>
                }
            } else {
                quote! {
                    #output_ident
                }
            }
        } else {
            quote! { () }
        };

        let error_type = if has_errors {
            let error_ident = syn::Ident::new(
                &format!("{}Error", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! { #error_ident<'de> }
        } else {
            quote! { jacquard_common::xrpc::GenericError<'de> }
        };

        // Generate the response type that implements XrpcResp
        let response_ident = syn::Ident::new(
            &format!("{}Response", type_base),
            proc_macro2::Span::call_site(),
        );

        // Generate the endpoint type that implements XrpcEndpoint
        let endpoint_ident = syn::Ident::new(
            &format!("{}Request", type_base),
            proc_macro2::Span::call_site(),
        );

        let decode_output_method = if output_encoding == "application/json" {
            quote! {}
        } else {
            let output_ident = syn::Ident::new(
                &format!("{}Output", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! {

                fn decode_output<'de>(body: &'de [u8]) -> Result<Self::Output<'de>, jacquard_common::error::DecodeError>
                where
                    Self::Output<'de>: serde::Deserialize<'de>,
                {
                    Ok(#output_ident {
                        body: bytes::Bytes::copy_from_slice(body),
                    })
                }
            }
        };

        let encode_output_method = if output_encoding == "application/json" {
            quote! {}
        } else {
            quote! {
                fn encode_output(output: &Self::Output<'_>) -> Result<Vec<u8>, jacquard_common::xrpc::EncodeError> {
                    Ok(output.body.to_vec())
                }
            }
        };

        let response_type = quote! {
            #[doc = " Response type for "]
            #[doc = #nsid]
            pub struct #response_ident;

            impl jacquard_common::xrpc::XrpcResp for #response_ident {
                const NSID: &'static str = #nsid;
                const ENCODING: &'static str = #output_encoding;
                type Output<'de> = #output_type;
                type Err<'de> = #error_type;

                #encode_output_method
                #decode_output_method
            }
        };

        // Generate encode_body() method for binary inputs
        let encode_body_method = if is_binary_input {
            quote! {
                fn encode_body(&self) -> Result<Vec<u8>, jacquard_common::xrpc::EncodeError> {
                    Ok(self.body.to_vec())
                }
            }
        } else {
            quote! {}
        };

        // Generate decode_body() method for binary inputs
        let decode_body_method = if is_binary_input {
            quote! {
                fn decode_body<'de>(
                    body: &'de [u8],
                ) -> Result<Box<Self>, jacquard_common::error::DecodeError>
                where
                    Self: serde::Deserialize<'de>,
                {
                    Ok(Box::new(Self {
                        body: bytes::Bytes::copy_from_slice(body),
                    }))
                }
            }
        } else {
            quote! {}
        };

        let endpoint_path = format!("/xrpc/{}", nsid);

        if has_params {
            // Implement on the params/input struct itself
            let request_ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

            let (impl_generics, impl_target, endpoint_request_type) = if params_has_lifetime {
                (
                    quote! { <'a> },
                    quote! { #request_ident<'a> },
                    quote! { #request_ident<'de> },
                )
            } else {
                (
                    quote! {},
                    quote! { #request_ident },
                    quote! { #request_ident },
                )
            };

            Ok(quote! {
                #response_type

                impl #impl_generics jacquard_common::xrpc::XrpcRequest for #impl_target {
                    const NSID: &'static str = #nsid;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Response = #response_ident;

                    #encode_body_method
                    #decode_body_method
                }

                #[doc = " Endpoint type for "]
                #[doc = #nsid]
                pub struct #endpoint_ident;

                impl jacquard_common::xrpc::XrpcEndpoint for #endpoint_ident {
                    const PATH: &'static str = #endpoint_path;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Request<'de> = #endpoint_request_type;
                    type Response = #response_ident;
                }
            })
        } else {
            // No params - generate a marker struct
            let request_ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

            Ok(quote! {
                /// XRPC request marker type
                #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, jacquard_derive::IntoStatic)]
                pub struct #request_ident;

                #response_type

                impl jacquard_common::xrpc::XrpcRequest for #request_ident {
                    const NSID: &'static str = #nsid;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Response = #response_ident;
                }

                #[doc = " Endpoint type for "]
                #[doc = #nsid]
                pub struct #endpoint_ident;

                impl jacquard_common::xrpc::XrpcEndpoint for #endpoint_ident {
                    const PATH: &'static str = #endpoint_path;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Request<'de> = #request_ident;
                    type Response = #response_ident;
                }
            })
        }
    }

    /// Generate XrpcSubscription trait impl for a subscription endpoint
    pub(super) fn generate_xrpc_subscription_impl(
        &self,
        nsid: &str,
        type_base: &str,
        has_params: bool,
        params_has_lifetime: bool,
        has_message: bool,
        has_errors: bool,
    ) -> Result<TokenStream> {
        // Generate stream response marker struct
        let stream_ident = syn::Ident::new(
            &format!("{}Stream", type_base),
            proc_macro2::Span::call_site(),
        );

        let message_type = if has_message {
            let msg_ident = syn::Ident::new(
                &format!("{}Message", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! { #msg_ident<'de> }
        } else {
            quote! { () }
        };

        let error_type = if has_errors {
            let err_ident = syn::Ident::new(
                &format!("{}Error", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! { #err_ident<'de> }
        } else {
            quote! { jacquard_common::xrpc::GenericError<'de> }
        };

        // Determine encoding from nsid convention
        // ATProto subscriptions use DAG-CBOR, community ones might use JSON
        let is_dag_cbor = nsid.starts_with("com.atproto");
        let encoding = if is_dag_cbor {
            quote! { jacquard_common::xrpc::MessageEncoding::DagCbor }
        } else {
            quote! { jacquard_common::xrpc::MessageEncoding::Json }
        };

        // Generate SubscriptionResp impl
        // For DAG-CBOR subscriptions, override decode_message to use framed decoding
        let decode_message_override = if is_dag_cbor && has_message {
            let msg_ident = syn::Ident::new(
                &format!("{}Message", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! {
                fn decode_message<'de>(bytes: &'de [u8]) -> Result<Self::Message<'de>, jacquard_common::error::DecodeError> {
                    #msg_ident::decode_framed(bytes)
                }
            }
        } else {
            quote! {}
        };

        let stream_resp_impl = quote! {
            #[doc = "Stream response type for "]
            #[doc = #nsid]
            pub struct #stream_ident;

            impl jacquard_common::xrpc::SubscriptionResp for #stream_ident {
                const NSID: &'static str = #nsid;
                const ENCODING: jacquard_common::xrpc::MessageEncoding = #encoding;

                type Message<'de> = #message_type;
                type Error<'de> = #error_type;

                #decode_message_override
            }
        };

        let params_ident = if has_params {
            syn::Ident::new(type_base, proc_macro2::Span::call_site())
        } else {
            // Generate marker struct if no params
            let marker = syn::Ident::new(type_base, proc_macro2::Span::call_site());
            let endpoint_ident = syn::Ident::new(
                &format!("{}Endpoint", type_base),
                proc_macro2::Span::call_site(),
            );
            let endpoint_path = format!("/xrpc/{}", nsid);

            return Ok(quote! {
                #stream_resp_impl

                #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
                pub struct #marker;

                impl jacquard_common::xrpc::XrpcSubscription for #marker {
                    const NSID: &'static str = #nsid;
                    const ENCODING: jacquard_common::xrpc::MessageEncoding = #encoding;

                    type Stream = #stream_ident;
                }

                pub struct #endpoint_ident;

                impl jacquard_common::xrpc::SubscriptionEndpoint for #endpoint_ident {
                    const PATH: &'static str = #endpoint_path;
                    const ENCODING: jacquard_common::xrpc::MessageEncoding = #encoding;

                    type Params<'de> = #marker;
                    type Stream = #stream_ident;
                }
            });
        };

        let (impl_generics, impl_target, endpoint_params_type) =
            if has_params && params_has_lifetime {
                (
                    quote! { <'a> },
                    quote! { #params_ident<'a> },
                    quote! { #params_ident<'de> },
                )
            } else {
                (
                    quote! {},
                    quote! { #params_ident },
                    quote! { #params_ident },
                )
            };

        let endpoint_ident = syn::Ident::new(
            &format!("{}Endpoint", type_base),
            proc_macro2::Span::call_site(),
        );

        let endpoint_path = format!("/xrpc/{}", nsid);

        Ok(quote! {
            #stream_resp_impl

            impl #impl_generics jacquard_common::xrpc::XrpcSubscription for #impl_target {
                const NSID: &'static str = #nsid;
                const ENCODING: jacquard_common::xrpc::MessageEncoding = #encoding;

                type Stream = #stream_ident;
            }

            pub struct #endpoint_ident;

            impl jacquard_common::xrpc::SubscriptionEndpoint for #endpoint_ident {
                const PATH: &'static str = #endpoint_path;
                const ENCODING: jacquard_common::xrpc::MessageEncoding = #encoding;

                type Params<'de> = #endpoint_params_type;
                type Stream = #stream_ident;
            }
        })
    }
}
