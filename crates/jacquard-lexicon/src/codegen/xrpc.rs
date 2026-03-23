use crate::error::Result;
use crate::lexicon::{
    LexXrpcBody, LexXrpcBodySchema, LexXrpcError, LexXrpcProcedure, LexXrpcQuery,
    LexXrpcSubscription, LexXrpcSubscriptionMessageSchema,
};
use heck::{ToPascalCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::quote;

use super::CodeGenerator;
use super::prettify::GeneratedCode;
use super::utils::make_ident;

impl<'c> CodeGenerator<'c> {
    /// Generate query type.
    pub(super) fn generate_query(
        &self,
        nsid: &str,
        def_name: &str,
        query: &LexXrpcQuery<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut type_defs_parts = Vec::new();
        let mut internals_parts: Vec<TokenStream> = Vec::new();

        let params_has_lifetime = query
            .parameters
            .as_ref()
            .map(|p| match p {
                crate::lexicon::LexXrpcQueryParameter::Params(params) => {
                    self.params_need_type_param(params)
                }
            })
            .unwrap_or(false);
        let has_params = query.parameters.is_some();
        let has_output = query.output.is_some();
        let has_errors = query.errors.is_some();

        if let Some(params) = &query.parameters {
            let sub = self.generate_params_struct(&type_base, nsid, params, resolved)?;
            type_defs_parts.push(sub.type_def);
            internals_parts.push(sub.default_fns);
            internals_parts.push(sub.builder);
        }

        if let Some(body) = &query.output {
            let sub = self.generate_output_struct(nsid, &type_base, body, resolved)?;
            type_defs_parts.push(sub.type_def);
            internals_parts.push(sub.default_fns);
            internals_parts.push(sub.builder);
        }

        if let Some(errors) = &query.errors {
            let error_enum = self.generate_error_enum(&type_base, errors, resolved)?;
            type_defs_parts.push(error_enum);
        }

        // Generate XrpcRequest impl.
        let output_encoding = query
            .output
            .as_ref()
            .map(|o| o.encoding.as_ref())
            .unwrap_or("application/json");
        let output_has_schema = query
            .output
            .as_ref()
            .map(|o| o.schema.is_some())
            .unwrap_or(false);

        let xrpc_impl = self.generate_xrpc_request_impl(
            nsid,
            &type_base,
            quote! { jacquard_common::xrpc::XrpcMethod::Query },
            output_encoding,
            has_params,
            params_has_lifetime,
            has_output,
            output_has_schema,
            has_errors,
            false, // queries never have binary inputs
            resolved,
        )?;

        // Categorize tokens into buckets.
        let type_defs = quote! { #(#type_defs_parts)* };
        let internals = quote! { #(#internals_parts)* };

        Ok(GeneratedCode {
            type_defs,
            inherent_impls: TokenStream::new(),
            trait_impls: xrpc_impl,
            internals,
            imports: Default::default(),
        })
    }

    /// Generate procedure type.
    pub(super) fn generate_procedure(
        &self,
        nsid: &str,
        def_name: &str,
        proc: &LexXrpcProcedure<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut type_defs_parts = Vec::new();
        let mut internals_parts: Vec<TokenStream> = Vec::new();

        // Check if input is a binary body (no schema).
        let is_binary_input = proc
            .input
            .as_ref()
            .map(|i| i.schema.is_none())
            .unwrap_or(false);

        // Input bodies with schemas have lifetimes (they get #[lexicon] attribute).
        // Binary inputs don't have lifetimes.
        let params_has_lifetime = proc.input.is_some() && !is_binary_input;
        let has_input = proc.input.is_some();
        let has_output = proc.output.is_some();
        let has_errors = proc.errors.is_some();

        if let Some(params) = &proc.parameters {
            let sub = self.generate_params_struct_proc(&type_base, nsid, params, resolved)?;
            type_defs_parts.push(sub.type_def);
            internals_parts.push(sub.default_fns);
            internals_parts.push(sub.builder);
        }

        if let Some(body) = &proc.input {
            let sub = self.generate_input_struct(nsid, &type_base, body, resolved)?;
            type_defs_parts.push(sub.type_def);
            internals_parts.push(sub.default_fns);
            internals_parts.push(sub.builder);
        }

        if let Some(body) = &proc.output {
            let sub = self.generate_output_struct(nsid, &type_base, body, resolved)?;
            type_defs_parts.push(sub.type_def);
            internals_parts.push(sub.default_fns);
            internals_parts.push(sub.builder);
        }

        if let Some(errors) = &proc.errors {
            let error_enum = self.generate_error_enum(&type_base, errors, resolved)?;
            type_defs_parts.push(error_enum);
        }

        // Generate XrpcRequest impl.
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
        let output_has_schema = proc
            .output
            .as_ref()
            .map(|o| o.schema.is_some())
            .unwrap_or(false);
        let xrpc_impl = self.generate_xrpc_request_impl(
            nsid,
            &type_base,
            quote! { jacquard_common::xrpc::XrpcMethod::Procedure(#input_encoding) },
            output_encoding,
            has_input,
            params_has_lifetime,
            has_output,
            output_has_schema,
            has_errors,
            is_binary_input,
            resolved,
        )?;

        // Categorize tokens into buckets.
        let type_defs = quote! { #(#type_defs_parts)* };
        let internals = quote! { #(#internals_parts)* };

        Ok(GeneratedCode {
            type_defs,
            inherent_impls: TokenStream::new(),
            trait_impls: xrpc_impl,
            internals,
            imports: Default::default(),
        })
    }

    /// Generate subscription type.
    pub(super) fn generate_subscription(
        &self,
        nsid: &str,
        def_name: &str,
        sub: &LexXrpcSubscription<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let type_base = self.def_to_type_name(nsid, def_name);
        let mut type_defs_parts = Vec::new();
        let mut internals_parts: Vec<TokenStream> = Vec::new();

        if let Some(params) = &sub.parameters {
            match params {
                crate::lexicon::LexXrpcSubscriptionParameter::Params(params_inner) => {
                    let sub_out = self.generate_params_struct_inner(
                        &type_base,
                        nsid,
                        params_inner,
                        resolved,
                    )?;
                    type_defs_parts.push(sub_out.type_def);
                    internals_parts.push(sub_out.default_fns);
                    internals_parts.push(sub_out.builder);
                }
            }
        }

        if let Some(message) = &sub.message {
            if let Some(schema) = &message.schema {
                let message_generated =
                    self.generate_subscription_message(nsid, &type_base, schema, resolved)?;
                type_defs_parts.push(message_generated.into_tokens());
            }
        }

        if let Some(errors) = &sub.errors {
            let error_enum = self.generate_error_enum(&type_base, errors, resolved)?;
            type_defs_parts.push(error_enum);
        }

        // Generate XrpcSubscription trait impl.
        let params_has_lifetime = sub
            .parameters
            .as_ref()
            .map(|p| match p {
                crate::lexicon::LexXrpcSubscriptionParameter::Params(params) => {
                    self.params_need_type_param(params)
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
            resolved,
        )?;

        // Categorize tokens into buckets.
        let type_defs = quote! { #(#type_defs_parts)* };
        let internals = quote! { #(#internals_parts)* };

        Ok(GeneratedCode {
            type_defs,
            inherent_impls: TokenStream::new(),
            trait_impls: subscription_impl,
            internals,
            imports: Default::default(),
        })
    }

    pub(super) fn generate_subscription_message(
        &self,
        nsid: &str,
        type_base: &str,
        schema: &LexXrpcSubscriptionMessageSchema<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        use crate::lexicon::LexXrpcSubscriptionMessageSchema;

        match schema {
            LexXrpcSubscriptionMessageSchema::Union(union) => {
                // Generate a union enum for the message
                let enum_name = format!("{}Message", type_base);
                let enum_ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

                // Build variants using the union_codegen module (simple mode, no collision detection)
                let ctx = super::union_codegen::UnionGenContext {
                    corpus: self.corpus,
                    namespace_deps: &self.namespace_deps,
                    current_nsid: nsid,
                };

                let union_variants = ctx.build_simple_union_variants(&union.refs, |ref_str| {
                    self.ref_to_rust_type(ref_str, resolved)
                })?;
                let variants = super::union_codegen::generate_variant_tokens(&union_variants);

                // Generate decode arms for framed decoding
                let decode_arms: Vec<_> = union_variants
                    .iter()
                    .map(|variant| {
                        let ref_str_literal = &variant.ref_str;
                        let variant_ident =
                            syn::Ident::new(&variant.variant_name, proc_macro2::Span::call_site());
                        quote! {
                            #ref_str_literal => {
                                let variant = jacquard_common::deps::codegen::serde_ipld_dagcbor::from_slice(body)?;
                                Ok(Self::#variant_ident(Box::new(variant)))
                            }
                        }
                    })
                    .collect();

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

                let open_union_attr =
                    resolved.attribute_tokens(&super::prettify::ExternalImport::OpenUnion);
                let derive_attr = resolved.derive_standard();
                let union_def = quote! {
                    #doc
                    #open_union_attr
                    #derive_attr
                    #[serde(tag = "$type")]
                    #[serde(bound(deserialize = "'de: 'a"))]
                    pub enum #enum_ident<'a> {
                        #(#variants,)*
                    }

                    #decode_framed_impl
                };
                Ok(GeneratedCode::type_only(union_def))
            }
            LexXrpcSubscriptionMessageSchema::Object(obj) => {
                // Generate a struct for the message
                let struct_name = format!("{}Message", type_base);
                let struct_ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

                let (fields, default_fns) =
                    self.generate_object_fields("", &struct_name, obj, false, resolved)?;
                let doc = self.generate_doc_comment(obj.description.as_ref());

                // Subscription message structs always get a lifetime since they have the #[lexicon] attribute
                // which adds extra_data: BTreeMap<..., Data<'a>>.
                let lexicon_attr =
                    resolved.attribute_tokens(&super::prettify::ExternalImport::LexiconAttr);
                let derive_attr = resolved.derive_standard();
                let struct_def = quote! {
                    #doc
                    #lexicon_attr
                    #derive_attr
                    #[serde(rename_all = "camelCase")]
                    pub struct #struct_ident<'a> {
                        #fields
                    }
                    #(#default_fns)*
                };

                // Generate union types for this message.
                let nested_items = self.generate_nested_types(
                    nsid,
                    &struct_name,
                    &obj.properties,
                    false,
                    resolved,
                )?;

                let mut nested_type_defs = TokenStream::new();
                let mut nested_internals = TokenStream::new();
                for nested in nested_items {
                    nested_type_defs.extend(nested.type_defs);
                    nested_internals.extend(nested.inherent_impls);
                    nested_internals.extend(nested.trait_impls);
                    nested_internals.extend(nested.internals);
                }

                Ok(GeneratedCode {
                    type_defs: quote! {
                        #struct_def
                        #nested_type_defs
                    },
                    inherent_impls: TokenStream::new(),
                    trait_impls: TokenStream::new(),
                    internals: nested_internals,
                    imports: Default::default(),
                })
            }
            LexXrpcSubscriptionMessageSchema::Ref(ref_type) => {
                // Type alias to the referenced type, parameterised on S if the ref needs it.
                let type_name = format!("{}Message", type_base);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let rust_type = self.ref_to_rust_type(&ref_type.r#ref, resolved)?;
                let doc = self.generate_doc_comment(ref_type.description.as_ref());

                let bosstr_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
                let default_str_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
                let type_alias = if self.ref_needs_type_param(&ref_type.r#ref) {
                    quote! {
                        #doc
                        pub type #ident<S: #bosstr_path = #default_str_path> = #rust_type;
                    }
                } else {
                    quote! {
                        #doc
                        pub type #ident = #rust_type;
                    }
                };
                Ok(GeneratedCode::type_only(type_alias))
            }
        }
    }

    /// Generate params struct from XRPC query parameters.
    pub(super) fn generate_params_struct(
        &self,
        type_base: &str,
        nsid: &str,
        params: &crate::lexicon::LexXrpcQueryParameter<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<super::prettify::SubGeneratorOutput> {
        use crate::lexicon::LexXrpcQueryParameter;
        match params {
            LexXrpcQueryParameter::Params(p) => {
                self.generate_params_struct_inner(type_base, nsid, p, resolved)
            }
        }
    }

    /// Generate params struct from XRPC procedure parameters (query string params).
    pub(super) fn generate_params_struct_proc(
        &self,
        type_base: &str,
        nsid: &str,
        params: &crate::lexicon::LexXrpcProcedureParameter<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<super::prettify::SubGeneratorOutput> {
        use crate::lexicon::LexXrpcProcedureParameter;
        match params {
            // For procedures, query string params still get "Params" suffix since the main struct is the input.
            LexXrpcProcedureParameter::Params(p) => {
                let struct_name = format!("{}Params", type_base);
                let ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());
                self.generate_params_struct_inner_with_name(&ident, nsid, p, resolved)
            }
        }
    }

    /// Generate params struct inner (shared implementation).
    pub(super) fn generate_params_struct_inner(
        &self,
        type_base: &str,
        nsid: &str,
        p: &crate::lexicon::LexXrpcParameters<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<super::prettify::SubGeneratorOutput> {
        let ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());
        self.generate_params_struct_inner_with_name(&ident, nsid, p, resolved)
    }

    /// Generate params struct with custom name.
    pub(super) fn generate_params_struct_inner_with_name(
        &self,
        ident: &syn::Ident,
        nsid: &str,
        p: &crate::lexicon::LexXrpcParameters<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<super::prettify::SubGeneratorOutput> {
        let required = p.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
        let mut fields = Vec::new();
        let mut default_fns = Vec::new();

        for (field_name, field_type) in &p.properties {
            let is_required = required.contains(field_name);
            let (field_tokens, default_fn) = self.generate_param_field_with_default(
                "",
                field_name,
                field_type,
                is_required,
                resolved,
            )?;
            fields.push(field_tokens);
            if let Some(fn_def) = default_fn {
                default_fns.push(fn_def);
            }
        }

        let doc = self.generate_doc_comment(p.description.as_ref());
        let needs_type_param = self.params_need_type_param(p);

        let derives = resolved.derive_standard();
        let bosstr_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
        let default_str_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
        let bosstr_serde =
            resolved.serde_external_path(&super::prettify::ExternalImport::BosStr);
        let ser_serde = resolved.serde_external_path(&super::prettify::ExternalImport::Serialize);
        let de_serde = resolved.serde_external_path(&super::prettify::ExternalImport::Deserialize);
        let serde_ser_bound = format!("S: {} + {}", ser_serde, bosstr_serde);
        // Since BosStr includes FromStaticStr, the serde bound is the same whether
        // or not there are string defaults with static values.
        let serde_de_bound = format!("S: {}<'de> + {}", de_serde, bosstr_serde);

        let struct_body = if fields.is_empty() {
            quote! {
                pub struct #ident;
            }
        } else if needs_type_param {
            quote! {
                pub struct #ident<S: #bosstr_path = #default_str_path> {
                    #(#fields)*
                }
            }
        } else {
            quote! {
                pub struct #ident {
                    #(#fields)*
                }
            }
        };

        let struct_def = if needs_type_param {
            quote! {
                #doc
                #derives
                #[serde(rename_all = "camelCase", bound(
                    serialize = #serde_ser_bound,
                    deserialize = #serde_de_bound
                ))]
                #struct_body
            }
        } else {
            quote! {
                #doc
                #derives
                #[serde(rename_all = "camelCase")]
                #struct_body
            }
        };


        let type_name = ident.to_string();
        let ctx = super::builder_gen::BuilderGenContext::from_parameters(
            self,
            nsid,
            &type_name,
            p,
            needs_type_param,
            resolved,
        );
        let builder = ctx.generate();

        Ok(super::prettify::SubGeneratorOutput {
            type_def: struct_def,
            default_fns: quote! { #(#default_fns)* },
            builder,
        })
    }

    /// Generate input struct from XRPC body.
    pub(super) fn generate_input_struct(
        &self,
        nsid: &str,
        type_base: &str,
        body: &LexXrpcBody<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<super::prettify::SubGeneratorOutput> {
        let ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

        // Check if this is a binary body (no schema, just raw bytes)
        let is_binary_body = body.schema.is_none();

        // Determine if we should derive Default or generate custom builder
        // Binary bodies skipped (single field), schema-based inputs use heuristics
        let (has_default, has_builder) = if is_binary_body {
            (false, false) // binary bodies don't get builder (single field)
        } else if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
            let decision = super::builder_heuristics::should_generate_builder(type_base, obj);
            (decision.has_default, decision.has_builder)
        } else {
            (false, false)
        };

        let bytes_type = resolved.external_type_tokens(&super::prettify::ExternalImport::Bytes);
        let (fields, default_fns) = if let Some(schema) = &body.schema {
            self.generate_body_fields(nsid, type_base, schema, has_builder, resolved)?
        } else {
            // Binary body: just a bytes field.
            (
                quote! {
                    pub body: #bytes_type,
                },
                Vec::new(),
            )
        };

        let doc = self.generate_doc_comment(body.description.as_ref());

        // Binary bodies have no type param. Schema bodies are parameterised on S.
        let struct_def = if is_binary_body {
            let derive_attr = resolved.derive_standard();
            quote! {
                #doc
                #derive_attr
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #fields
                }
            }
        } else {
            let derive_attr = if has_default {
                resolved.derive_standard_with(quote! { Default })
            } else {
                resolved.derive_standard()
            };
            let bosstr_path =
                resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
            let default_str_path =
                resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
            let bosstr_serde =
                resolved.serde_external_path(&super::prettify::ExternalImport::BosStr);
            let ser_serde =
                resolved.serde_external_path(&super::prettify::ExternalImport::Serialize);
            let de_serde =
                resolved.serde_external_path(&super::prettify::ExternalImport::Deserialize);
            let serde_ser_bound = format!("S: {} + {}", ser_serde, bosstr_serde);
            let serde_de_bound = format!("S: {}<'de> + {}", de_serde, bosstr_serde);
            let data_type = resolved.type_tokens(&super::prettify::CommonType::Data);
            let smol_str_type = resolved.type_tokens(&super::prettify::CommonType::SmolStr);
            let btree_map = resolved.btree_map_path();
            let is_none_path = resolved.option_is_none_path();
            let extra_data_type =
                resolved.option_type(quote! { #btree_map<#smol_str_type, #data_type> });
            quote! {
                #doc
                #derive_attr
                #[serde(rename_all = "camelCase", bound(
                    serialize = #serde_ser_bound,
                    deserialize = #serde_de_bound
                ))]
                pub struct #ident<S: #bosstr_path = #default_str_path> {
                    #fields
                    #[serde(flatten, default, skip_serializing_if = #is_none_path)]
                    pub extra_data: #extra_data_type,
                }
            }
        };

        // Generate custom builder if needed (binary bodies skipped — single field).
        let builder = if !is_binary_body && has_builder {
            if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
                let ctx = super::builder_gen::BuilderGenContext::from_object(
                    self, nsid, type_base, obj,
                    true, // input structs always have lifetime (for #[lexicon])
                    resolved,
                );
                ctx.generate()
            } else {
                quote! {}
            }
        } else {
            quote! {}
        };

        // Generate union types if schema is an Object.
        let nested_items =
            if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
                self.generate_nested_types(nsid, type_base, &obj.properties, false, resolved)?
            } else {
                Vec::new()
            };

        let mut nested_type_defs = TokenStream::new();
        let mut nested_internals = TokenStream::new();
        for nested in nested_items {
            nested_type_defs.extend(nested.type_defs);
            nested_internals.extend(nested.inherent_impls);
            nested_internals.extend(nested.trait_impls);
            nested_internals.extend(nested.internals);
        }

        Ok(super::prettify::SubGeneratorOutput {
            type_def: quote! {
                #struct_def
                #nested_type_defs
            },
            default_fns: quote! { #(#default_fns)* },
            builder: quote! {
                #nested_internals
                #builder
            },
        })
    }

    /// Generate output struct from XRPC body.
    pub(super) fn generate_output_struct(
        &self,
        nsid: &str,
        type_base: &str,
        body: &LexXrpcBody<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<super::prettify::SubGeneratorOutput> {
        let struct_name = format!("{}Output", type_base);
        let ident = syn::Ident::new(&struct_name, proc_macro2::Span::call_site());

        let bytes_type = resolved.external_type_tokens(&super::prettify::ExternalImport::Bytes);
        let (fields, default_fns) = if let Some(schema) = &body.schema {
            self.generate_body_fields(nsid, &struct_name, schema, false, resolved)?
        } else {
            (
                quote! {
                    pub body: #bytes_type,
                },
                Vec::new(),
            )
        };

        let doc = self.generate_doc_comment(body.description.as_ref());

        // Determine if we should derive Default.
        // Check if schema is an Object and apply heuristics.
        let has_default = if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema
        {
            super::builder_heuristics::should_generate_builder(&struct_name, obj).has_default
        } else {
            false
        };

        // Output structs: binary outputs have no type param, schema outputs
        // are parameterised on S with extra_data emitted directly.
        let struct_def = if body.schema.is_none() {
            // Binary output: no type param, no extra_data.
            let derive_attr = resolved.derive_standard();
            quote! {
                #doc
                #derive_attr
                #[serde(rename_all = "camelCase")]
                pub struct #ident {
                    #fields
                }
            }
        } else {
            let derive_attr = if has_default {
                resolved.derive_standard_with(quote! { Default })
            } else {
                resolved.derive_standard()
            };
            let bosstr_path =
                resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
            let default_str_path =
                resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
            let bosstr_serde =
                resolved.serde_external_path(&super::prettify::ExternalImport::BosStr);
            let ser_serde =
                resolved.serde_external_path(&super::prettify::ExternalImport::Serialize);
            let de_serde =
                resolved.serde_external_path(&super::prettify::ExternalImport::Deserialize);
            let serde_ser_bound = format!("S: {} + {}", ser_serde, bosstr_serde);
            let serde_de_bound = format!("S: {}<'de> + {}", de_serde, bosstr_serde);
            let data_type = resolved.type_tokens(&super::prettify::CommonType::Data);
            let smol_str_type = resolved.type_tokens(&super::prettify::CommonType::SmolStr);
            let btree_map = resolved.btree_map_path();
            let is_none_path = resolved.option_is_none_path();
            let extra_data_type =
                resolved.option_type(quote! { #btree_map<#smol_str_type, #data_type> });
            quote! {
                #doc
                #derive_attr
                #[serde(rename_all = "camelCase", bound(
                    serialize = #serde_ser_bound,
                    deserialize = #serde_de_bound
                ))]
                pub struct #ident<S: #bosstr_path = #default_str_path> {
                    #fields
                    #[serde(flatten, default, skip_serializing_if = #is_none_path)]
                    pub extra_data: #extra_data_type,
                }
            }
        };

        // Generate union types if schema is an Object.
        let nested_items =
            if let Some(crate::lexicon::LexXrpcBodySchema::Object(obj)) = &body.schema {
                self.generate_nested_types(nsid, &struct_name, &obj.properties, false, resolved)?
            } else {
                Vec::new()
            };

        let mut nested_type_defs = TokenStream::new();
        let mut nested_internals = TokenStream::new();
        for nested in nested_items {
            nested_type_defs.extend(nested.type_defs);
            nested_internals.extend(nested.inherent_impls);
            nested_internals.extend(nested.trait_impls);
            nested_internals.extend(nested.internals);
        }

        // Output structs don't generate builders (they're response types).
        Ok(super::prettify::SubGeneratorOutput {
            type_def: quote! {
                #struct_def
                #nested_type_defs
            },
            default_fns: quote! { #(#default_fns)* },
            builder: nested_internals,
        })
    }

    /// Generate fields from XRPC body schema.
    /// Returns (field tokens, companion default functions).
    pub(super) fn generate_body_fields(
        &self,
        nsid: &str,
        parent_type_name: &str,
        schema: &LexXrpcBodySchema<'static>,
        is_builder: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<(TokenStream, Vec<TokenStream>)> {
        use crate::lexicon::LexXrpcBodySchema;

        match schema {
            LexXrpcBodySchema::Object(obj) => {
                self.generate_object_fields(nsid, parent_type_name, obj, is_builder, resolved)
            }
            LexXrpcBodySchema::Ref(ref_type) => {
                let rust_type = self.ref_to_rust_type(&ref_type.r#ref, resolved)?;
                Ok((
                    quote! {
                        #[serde(flatten)]
                        pub value: #rust_type,
                    },
                    Vec::new(),
                ))
            }
            LexXrpcBodySchema::Union(_union) => {
                let data_type = resolved.type_tokens(&super::prettify::CommonType::Data);
                Ok((
                    quote! {
                        #[serde(flatten)]
                        pub value: #data_type,
                    },
                    Vec::new(),
                ))
            }
        }
    }

    /// Generate a field for XRPC parameters.
    pub(super) fn generate_param_field(
        &self,
        _nsid: &str,
        field_name: &str,
        field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
        is_required: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<TokenStream> {
        use crate::lexicon::LexXrpcParametersProperty;

        let field_ident = make_ident(&field_name.to_snake_case());

        let data_type = resolved.type_tokens(&super::prettify::CommonType::Data);
        let (rust_type, _needs_type_param) = match field_type {
            LexXrpcParametersProperty::Boolean(_) => (quote! { bool }, false),
            LexXrpcParametersProperty::Integer(_) => (quote! { i64 }, false),
            LexXrpcParametersProperty::String(s) => (
                self.string_to_rust_type(s, resolved),
                self.string_needs_type_param(s),
            ),
            LexXrpcParametersProperty::Unknown(_) => (data_type.clone(), true),
            LexXrpcParametersProperty::Array(arr) => {
                let needs_type_param = match &arr.items {
                    crate::lexicon::LexPrimitiveArrayItem::Boolean(_)
                    | crate::lexicon::LexPrimitiveArrayItem::Integer(_) => false,
                    crate::lexicon::LexPrimitiveArrayItem::String(s) => {
                        self.string_needs_type_param(s)
                    }
                    crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => true,
                };
                let item_type = match &arr.items {
                    crate::lexicon::LexPrimitiveArrayItem::Boolean(_) => quote! { bool },
                    crate::lexicon::LexPrimitiveArrayItem::Integer(_) => quote! { i64 },
                    crate::lexicon::LexPrimitiveArrayItem::String(s) => {
                        self.string_to_rust_type(s, resolved)
                    }
                    crate::lexicon::LexPrimitiveArrayItem::Unknown(_) => data_type.clone(),
                };
                (quote! { Vec<#item_type> }, needs_type_param)
            }
        };

        let rust_type = if is_required {
            rust_type
        } else {
            resolved.option_type(rust_type)
        };

        let mut attrs = Vec::new();

        if !is_required {
            let is_none_path = resolved.option_is_none_path();
            attrs.push(quote! { #[serde(skip_serializing_if = #is_none_path)] });
        }

        Ok(quote! {
            #(#attrs)*
            pub #field_ident: #rust_type,
        })
    }

    /// Generate param field with serde default if present.
    /// Returns (field_tokens, optional_default_function).
    pub(super) fn generate_param_field_with_default(
        &self,
        nsid: &str,
        field_name: &str,
        field_type: &crate::lexicon::LexXrpcParametersProperty<'static>,
        is_required: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<(TokenStream, Option<TokenStream>)> {
        use crate::lexicon::LexXrpcParametersProperty;
        use heck::ToSnakeCase;

        // Get base field.
        let base_field =
            self.generate_param_field(nsid, field_name, field_type, is_required, resolved)?;

        // Generate default function and attribute for required fields with defaults
        // For optional fields, just add doc comments
        let (doc_comment, serde_attr, default_fn) = if is_required {
            match field_type {
                LexXrpcParametersProperty::Boolean(b) if b.default.is_some() => {
                    let v = b.default.unwrap();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!(" Defaults to `{}`", v)),
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
                        Some(format!(" Defaults to `{}`", v)),
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
                        Some(format!("Defaults to `\"{}\"`.", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident<S: jacquard_common::FromStaticStr>() -> S {
                                S::from_static(#v)
                            }
                        }),
                    )
                }
                _ => (None, None, None),
            }
        } else {
            // Optional fields with defaults get serde(default) returning Option<T>.
            match field_type {
                LexXrpcParametersProperty::Boolean(b) if b.default.is_some() => {
                    let v = b.default.unwrap();
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(format!(" Defaults to `{}`.", v)),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        {
                            let opt_bool = resolved.option_type(quote! { bool });
                            Some(quote! {
                                fn #fn_ident() -> #opt_bool { Some(#v) }
                            })
                        },
                    )
                }
                LexXrpcParametersProperty::Integer(i) if i.default.is_some() => {
                    let v = i.default.unwrap();
                    let mut parts = Vec::new();
                    parts.push(format!("Defaults to `{}`.", v));
                    if let Some(min) = i.minimum {
                        parts.push(format!("Min: {}.", min));
                    }
                    if let Some(max) = i.maximum {
                        parts.push(format!("Max: {}.", max));
                    }
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(parts.join(" ")),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        {
                            let opt_i64 = resolved.option_type(quote! { i64 });
                            Some(quote! {
                                fn #fn_ident() -> #opt_i64 { Some(#v) }
                            })
                        },
                    )
                }
                LexXrpcParametersProperty::String(s) if s.default.is_some() => {
                    let v = s.default.as_ref().unwrap().as_ref();
                    let mut parts = Vec::new();
                    parts.push(format!("Defaults to `\"{}\"`.", v));
                    if let Some(min) = s.min_length {
                        parts.push(format!("Min length: {}.", min));
                    }
                    if let Some(max) = s.max_length {
                        parts.push(format!("Max length: {}.", max));
                    }
                    let fn_name = format!("_default_{}", field_name.to_snake_case());
                    let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
                    (
                        Some(parts.join(" ")),
                        Some(quote! { #[serde(default = #fn_name)] }),
                        Some(quote! {
                            fn #fn_ident<S: jacquard_common::FromStaticStr>() -> Option<S> {
                                Some(S::from_static(#v))
                            }
                        }),
                    )
                }
                // Optional fields without defaults: doc comments only.
                LexXrpcParametersProperty::Integer(i) => {
                    let mut parts = Vec::new();
                    if let Some(min) = i.minimum {
                        parts.push(format!("min: {}", min));
                    }
                    if let Some(max) = i.maximum {
                        parts.push(format!("max: {}", max));
                    }
                    let doc = if !parts.is_empty() {
                        Some(format!("({})", parts.join(", ")))
                    } else {
                        None
                    };
                    (doc, None, None)
                }
                LexXrpcParametersProperty::String(s) => {
                    let mut parts = Vec::new();
                    if let Some(min) = s.min_length {
                        parts.push(format!("min length: {}", min));
                    }
                    if let Some(max) = s.max_length {
                        parts.push(format!("max length: {}", max));
                    }
                    let doc = if !parts.is_empty() {
                        Some(format!("({})", parts.join(", ")))
                    } else {
                        None
                    };
                    (doc, None, None)
                }
                _ => (None, None, None),
            }
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

    /// Generate error enum from XRPC errors.
    pub(super) fn generate_error_enum(
        &self,
        type_base: &str,
        errors: &[LexXrpcError<'static>],
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<TokenStream> {
        let enum_name = format!("{}Error", type_base);
        let ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

        let mut variants = Vec::new();
        let mut display_arms = Vec::new();

        let smol_str_type = resolved.type_tokens(&super::prettify::CommonType::SmolStr);

        for error in errors {
            let variant_name = error.name.to_pascal_case();
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            let error_name = error.name.as_ref();
            let doc = self.generate_doc_comment(error.description.as_ref());

            let opt_smolstr = resolved.option_type(smol_str_type.clone());
            variants.push(quote! {
                #doc
                #[serde(rename = #error_name)]
                #variant_ident(#opt_smolstr)
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

        // Error types are always owned (SmolStr-backed, no lifetime).
        // No #[open_union] — we emit the catch-all variant directly.
        let derive_attr = resolved.derive_error();

        Ok(quote! {
            #derive_attr
            #[serde(tag = "error", content = "message")]
            pub enum #ident {
                #(#variants,)*
                /// Catch-all for unknown error codes.
                #[serde(untagged)]
                Other { error: #smol_str_type, message: Option<#smol_str_type> },
            }

            impl core::fmt::Display for #ident {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    match self {
                        #(#display_arms)*
                        Self::Other { error, message } => {
                            write!(f, "{}", error)?;
                            if let Some(msg) = message {
                                write!(f, ": {}", msg)?;
                            }
                            Ok(())
                        }
                    }
                }
            }
        })
    }

    /// Generate XrpcRequest trait impl for a query or procedure.
    pub(super) fn generate_xrpc_request_impl(
        &self,
        nsid: &str,
        type_base: &str,
        method: TokenStream,
        output_encoding: &str,
        has_params: bool,
        params_has_lifetime: bool,
        has_output: bool,
        output_has_schema: bool,
        has_errors: bool,
        is_binary_input: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<TokenStream> {
        let output_type = if has_output {
            let output_ident = syn::Ident::new(
                &format!("{}Output", type_base),
                proc_macro2::Span::call_site(),
            );
            // Schema outputs get S param, binary outputs don't.
            if output_has_schema {
                quote! { #output_ident<S> }
            } else {
                quote! { #output_ident }
            }
        } else {
            quote! { () }
        };

        let error_type = if has_errors {
            let error_ident = syn::Ident::new(
                &format!("{}Error", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! { #error_ident }
        } else {
            quote! { jacquard_common::xrpc::GenericError }
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

        let bosstr_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
        let ser_path = resolved.external_type_tokens(&super::prettify::ExternalImport::Serialize);
        let de_path = resolved.external_type_tokens(&super::prettify::ExternalImport::Deserialize);
        let decode_error_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::DecodeError);
        let encode_error_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::EncodeError);

        let decode_output_method = if output_encoding == "application/json" {
            quote! {}
        } else {
            let output_ident = syn::Ident::new(
                &format!("{}Output", type_base),
                proc_macro2::Span::call_site(),
            );
            quote! {
                fn decode_output<'de, S>(body: &'de [u8]) -> Result<Self::Output<S>, #decode_error_path>
                where
                    S: #bosstr_path + #de_path<'de>,
                    Self::Output<S>: #de_path<'de>,
                {
                    Ok(#output_ident {
                        body: jacquard_common::deps::bytes::Bytes::copy_from_slice(body),
                    })
                }
            }
        };

        let encode_output_method = if output_encoding == "application/json" {
            quote! {}
        } else {
            quote! {
                fn encode_output<S: #bosstr_path>(
                    output: &Self::Output<S>,
                ) -> Result<Vec<u8>, #encode_error_path>
                where Self::Output<S>: #ser_path
                {
                    Ok(output.body.to_vec())
                }
            }
        };

        let nsid_str = format!(" Response type for {}", nsid);
        let response_type = quote! {
            #[doc = #nsid_str]
            pub struct #response_ident;

            impl jacquard_common::xrpc::XrpcResp for #response_ident {
                const NSID: &'static str = #nsid;
                const ENCODING: &'static str = #output_encoding;
                type Output<S: #bosstr_path> = #output_type;
                type Err = #error_type;

                #encode_output_method
                #decode_output_method
            }
        };

        // Generate encode_body() method for binary inputs
        let encode_body_method = if is_binary_input {
            quote! {
                fn encode_body(&self, buffer: &mut [u8]) -> Result<(), jacquard_common::xrpc::EncodeError>
                where
                    Self: #ser_path,
                {
                    Ok(
                        buffer.copy_from_slice(self.body.as_ref())
                    )
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
                ) -> Result<Self, jacquard_common::error::DecodeError>
                where
                    Self: #de_path<'de>,
                {
                    Ok(Self {
                        body: jacquard_common::deps::bytes::Bytes::copy_from_slice(body),
                    })
                }
            }
        } else {
            quote! {}
        };

        let endpoint_path = format!("/xrpc/{}", nsid);
        let marker_derive = resolved.derive_standard_with(quote! { Copy });

        if has_params {
            // Implement on the params/input struct itself
            let request_ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

            let (impl_generics, impl_target, endpoint_request_type) = if params_has_lifetime {
                (
                    quote! { <S: #bosstr_path> },
                    //quote! { <S: #bosstr_path + #ser_path> },
                    quote! { #request_ident<S> },
                    quote! { #request_ident<S> },
                )
            } else {
                (
                    quote! {},
                    quote! { #request_ident },
                    quote! { #request_ident },
                )
            };
            let nsid_str = format!(" Endpoint type for {}", nsid);

            Ok(quote! {
                #response_type

                impl #impl_generics jacquard_common::xrpc::XrpcRequest for #impl_target {
                    const NSID: &'static str = #nsid;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Response = #response_ident;

                    #encode_body_method
                    #decode_body_method
                }


                #[doc = #nsid_str]
                pub struct #endpoint_ident;

                impl jacquard_common::xrpc::XrpcEndpoint for #endpoint_ident {
                    const PATH: &'static str = #endpoint_path;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Request<S: #bosstr_path> = #endpoint_request_type;
                    type Response = #response_ident;
                }
            })
        } else {
            // No params - generate a marker struct
            let request_ident = syn::Ident::new(type_base, proc_macro2::Span::call_site());

            let nsid_str = format!(" Endpoint type for {}", nsid);
            Ok(quote! {
                /// XRPC request marker type.
                #marker_derive
                pub struct #request_ident;

                #response_type

                impl jacquard_common::xrpc::XrpcRequest for #request_ident {
                    const NSID: &'static str = #nsid;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Response = #response_ident;
                }

                #[doc = #nsid_str]
                pub struct #endpoint_ident;

                impl jacquard_common::xrpc::XrpcEndpoint for #endpoint_ident {
                    const PATH: &'static str = #endpoint_path;
                    const METHOD: jacquard_common::xrpc::XrpcMethod = #method;

                    type Request<S: #bosstr_path> = #request_ident;
                    type Response = #response_ident;
                }
            })
        }
    }

    /// Generate XrpcSubscription trait impl for a subscription endpoint.
    pub(super) fn generate_xrpc_subscription_impl(
        &self,
        nsid: &str,
        type_base: &str,
        has_params: bool,
        params_has_lifetime: bool,
        has_message: bool,
        has_errors: bool,
        resolved: &super::prettify::ResolvedImports,
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
            quote! { #err_ident }
        } else {
            quote! { jacquard_common::xrpc::GenericError }
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
                type Error = #error_type;

                #decode_message_override
            }
        };

        let ser_path = resolved.external_type_tokens(&super::prettify::ExternalImport::Serialize);
        let sub_marker_derive = quote! { #[derive(Debug, Clone, Copy, PartialEq, Eq, #ser_path)] };

        let params_ident = if has_params {
            syn::Ident::new(type_base, proc_macro2::Span::call_site())
        } else {
            // Generate marker struct if no params.
            let marker = syn::Ident::new(type_base, proc_macro2::Span::call_site());
            let endpoint_ident = syn::Ident::new(
                &format!("{}Endpoint", type_base),
                proc_macro2::Span::call_site(),
            );
            let endpoint_path = format!("/xrpc/{}", nsid);

            return Ok(quote! {
                #stream_resp_impl

                #sub_marker_derive
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
