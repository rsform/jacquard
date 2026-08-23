use crate::error::Result;
use crate::lexicon::{
    LexArrayItem, LexInteger, LexObject, LexObjectProperty, LexRecord, LexString,
};
use heck::ToSnakeCase;
use jacquard_common::{CowStr, deps::smol_str::SmolStr};
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeMap;

use super::CodeGenerator;
use super::prettify::GeneratedCode;
use super::utils::{
    generate_doc_comment, known_value_to_variant_name, make_ident, string_enum_is_nameable,
    value_to_variant_name,
};

/// Enum variant kind for IntoStatic generation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) enum EnumVariantKind {
    Unit,
    Tuple,
    Struct(Vec<String>),
}

impl<'c> CodeGenerator<'c> {
    /// Generate all nested type definitions (unions, objects) for an object's properties.
    /// This consolidates the pattern of iterating properties to find unions and nested objects
    /// that need their own type definitions.
    ///
    /// # Parameters
    /// - `include_nested_objects`: If false, skips generating nested object types (used by XRPC)
    pub(super) fn generate_nested_types(
        &self,
        nsid: &str,
        parent_type_name: &str,
        properties: &BTreeMap<SmolStr, LexObjectProperty<'static>>,
        include_nested_objects: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<Vec<GeneratedCode>> {
        let mut nested = Vec::new();

        for (field_name, field_type) in properties {
            match field_type {
                LexObjectProperty::Union(union) => {
                    // Skip empty, single-variant unions unless they're self-referential.
                    if !union.refs.is_empty()
                        && (union.refs.len() > 1
                            || self.is_self_referential_union(nsid, parent_type_name, &union))
                    {
                        let union_name =
                            self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                        let refs: Vec<_> = union.refs.iter().cloned().collect();
                        nested.push(self.generate_union(
                            nsid,
                            &union_name,
                            &refs,
                            None,
                            union.closed,
                            resolved,
                        )?);
                    }
                }
                LexObjectProperty::Object(nested_obj) if include_nested_objects => {
                    let object_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    nested.push(self.generate_object(nsid, &object_name, &nested_obj, resolved)?);
                }
                LexObjectProperty::Array(array) => {
                    if let LexArrayItem::Union(union) = &array.items {
                        // Skip single-variant array unions.
                        if union.refs.len() > 1 {
                            let union_name = self.generate_field_type_name(
                                nsid,
                                parent_type_name,
                                field_name,
                                "Item",
                            );
                            let refs: Vec<_> = union.refs.iter().cloned().collect();
                            nested.push(self.generate_union(
                                nsid,
                                &union_name,
                                &refs,
                                None,
                                union.closed,
                                resolved,
                            )?);
                        }
                    } else if let LexArrayItem::String(s) = &array.items {
                        if s.r#enum.is_some() || s.known_values.is_some() {
                            let enum_name = self.generate_field_type_name(
                                nsid,
                                parent_type_name,
                                field_name,
                                "",
                            );
                            nested.push(self.generate_inline_string_enum(&enum_name, s, resolved)?);
                        }
                    }
                }
                LexObjectProperty::String(s) if s.known_values.is_some() || s.r#enum.is_some() => {
                    let enum_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    nested.push(self.generate_inline_string_enum(&enum_name, s, resolved)?);
                }
                _ => {}
            }
        }

        Ok(nested)
    }

    pub(super) fn generate_record(
        &self,
        nsid: &str,
        def_name: &str,
        record: &LexRecord<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        match &record.record {
            crate::lexicon::LexRecordRecord::Object(obj) => {
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

                // Records always get a type param since they have extra_data: BTreeMap<..., Data<S>>.
                // Skip custom builder for types that conflict with the macro's unqualified type references.
                let has_builder =
                    !super::builder_heuristics::conflicts_with_builder_macro(&type_name);

                // Generate main struct fields.
                let (fields, default_fns) =
                    self.generate_object_fields(nsid, &type_name, obj, has_builder, resolved)?;
                let doc = self.generate_doc_comment(record.description.as_ref());
                let manual_default = self.generate_manual_default(nsid, &type_name, obj, resolved);

                let derive_attr = resolved.derive_standard();
                let bosstr_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
                let default_str_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
                let bosstr_serde =
                    resolved.serde_external_path(&super::prettify::ExternalImport::BosStr);
                let de_serde =
                    resolved.serde_external_path(&super::prettify::ExternalImport::Deserialize);
                let serde_de_bound = format!("S: {}<'de> + {}", de_serde, bosstr_serde);

                // Generate the extra_data field directly instead of using #[lexicon] attribute.
                let smolstr_type = resolved.type_tokens(&super::prettify::CommonType::SmolStr);
                let data_type = resolved.type_tokens(&super::prettify::CommonType::Data);
                let btree_map = resolved.btree_map_path();
                let is_none_path = resolved.option_is_none_path();
                let extra_data_type =
                    resolved.option_type(quote! { #btree_map<#smolstr_type, #data_type> });
                let deserialize_extra_data_name =
                    format!("deserialize_{}_extra_data", type_name.to_snake_case());
                let deserialize_extra_data =
                    syn::Ident::new(&deserialize_extra_data_name, proc_macro2::Span::call_site());
                let struct_def = quote! {
                    #doc
                    #derive_attr
                    #[serde(rename_all = "camelCase", rename = #nsid, tag = "$type", bound(
                        deserialize = #serde_de_bound
                    ))]
                    pub struct #ident<S: #bosstr_path = #default_str_path> {
                        #fields
                        #[serde(
                            flatten,
                            default,
                            deserialize_with = #deserialize_extra_data_name,
                            skip_serializing_if = #is_none_path
                        )]
                        pub extra_data: #extra_data_type,
                    }
                };
                let extra_data_deserializer = quote! {
                    fn #deserialize_extra_data<'de, S, D>(
                        deserializer: D,
                    ) -> Result<#extra_data_type, D::Error>
                    where
                        S: #bosstr_path + serde::Deserialize<'de>,
                        D: serde::Deserializer<'de>,
                    {
                        let mut data = <#extra_data_type as serde::Deserialize<'de>>::deserialize(
                            deserializer,
                        )?;
                        if let Some(extra_data) = &mut data {
                            extra_data.remove("$type");
                            if extra_data.is_empty() {
                                data = None;
                            }
                        }
                        Ok(data)
                    }
                };

                // Generate custom builder if needed.
                let builder = if has_builder {
                    let ctx = super::builder_gen::BuilderGenContext::from_object(
                        self, nsid, &type_name, obj, true, // records always have type param
                        resolved,
                    );
                    ctx.generate()
                } else {
                    quote! {}
                };

                // Generate union types and nested object types for this record.
                let unions =
                    self.generate_nested_types(nsid, &type_name, &obj.properties, true, resolved)?;

                // Generate typed GetRecordOutput wrapper.
                let output_type_name = format!("{}GetRecordOutput", type_name);
                let output_type_ident =
                    syn::Ident::new(&output_type_name, proc_macro2::Span::call_site());

                let is_none_path = resolved.option_is_none_path();
                let cid_type = resolved.type_tokens(&super::prettify::CommonType::Cid);
                let at_uri_type = resolved.type_tokens(&super::prettify::CommonType::AtUri);
                let option_cid = resolved.option_type(cid_type);
                let output_wrapper = quote! {
                    /// Typed wrapper for GetRecord response with this collection's record type.
                    #derive_attr
                    #[serde(rename_all = "camelCase")]
                    pub struct #output_type_ident<S: #bosstr_path = #default_str_path> {
                        #[serde(skip_serializing_if = #is_none_path)]
                        pub cid: #option_cid,
                        pub uri: #at_uri_type,
                        pub value: #ident<S>,
                    }
                };

                // Generate marker struct for XrpcResp.
                let record_marker_name = format!("{}Record", type_name);
                let record_marker_ident =
                    syn::Ident::new(&record_marker_name, proc_macro2::Span::call_site());

                let ser_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::Serialize);
                let de_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::Deserialize);
                let xrpc_resp_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::XrpcResp);
                let record_error_type =
                    resolved.type_tokens(&super::prettify::CommonType::RecordError);
                let record_marker = quote! {
                    /// Marker type for deserializing records from this collection.
                    #[derive(Debug, #ser_path, #de_path)]
                    pub struct #record_marker_ident;

                    impl #xrpc_resp_path for #record_marker_ident {
                        const NSID: &'static str = #nsid;
                        const ENCODING: &'static str = "application/json";
                        type Output<S: #bosstr_path> = #output_type_ident<S>;
                        type Err = #record_error_type;
                    }
                };
                let from_impl = quote! {
                    impl<S: #bosstr_path> From<#output_type_ident<S>> for #ident<S> {
                        fn from(output: #output_type_ident<S>) -> Self {
                            output.value
                        }
                    }
                };

                // Generate Collection trait impl.
                let collection_path = resolved.type_path(&super::prettify::CommonType::Collection);
                let collection_impl = quote! {
                    impl<S: #bosstr_path> #collection_path for #ident<S> {
                        const NSID: &'static str = #nsid;
                        type Record = #record_marker_ident;
                    }
                };

                // Generate collection impl for the marker struct to drive fetch_record().
                let collection_marker_impl = quote! {
                    impl #collection_path for #record_marker_ident {
                        const NSID: &'static str = #nsid;
                        type Record = #record_marker_ident;
                    }
                };

                // Generate LexiconSchema impl with shared lexicon_doc function
                let (shared_fn, schema_impl) =
                    self.generate_schema_impl_with_shared(&type_name, nsid, "main", true, resolved);

                // Merge nested type buckets into parent buckets.
                let mut nested_type_defs = TokenStream::new();
                let mut nested_internals = TokenStream::new();
                for nested in unions {
                    nested_type_defs.extend(nested.type_defs);
                    nested_internals.extend(nested.inherent_impls);
                    nested_internals.extend(nested.trait_impls);
                    nested_internals.extend(nested.internals);
                }

                // Categorize tokens into buckets.
                let type_defs = quote! {
                    #struct_def
                    #nested_type_defs
                    #output_wrapper
                };

                let at_uri_path = resolved.type_path(&super::prettify::CommonType::AtUri);
                let record_uri_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::RecordUri);
                let uri_error_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::UriError);
                let inherent_impls = quote! {
                    impl<S: #bosstr_path> #ident<S> {
                        pub fn uri(uri: S) -> Result<#record_uri_path<S, #record_marker_ident>, #uri_error_path> {
                            #record_uri_path::try_from_uri(#at_uri_path::new(uri)?)
                        }
                    }
                };

                let trait_impls = quote! {
                    #record_marker
                    #from_impl
                    #collection_impl
                    #collection_marker_impl
                    #schema_impl
                };

                let internals = quote! {
                    #extra_data_deserializer
                    #(#default_fns)*
                    #manual_default
                    #nested_internals
                    #builder
                    #shared_fn
                };

                Ok(GeneratedCode {
                    type_defs,
                    inherent_impls,
                    trait_impls,
                    internals,
                    imports: Default::default(),
                })
            }
        }
    }

    /// Generate an object type
    pub(super) fn generate_object(
        &self,
        nsid: &str,
        def_name: &str,
        obj: &LexObject<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        // Objects always get a type param since they have extra_data: BTreeMap<..., Data<S>>.

        // Smart heuristics for builder generation:
        // - 0 required fields: Default instead of builder
        // - All required fields are bare strings: Default instead of builder
        // - 1+ required fields (not all strings): custom builder (but not if name conflicts)
        let decision = super::builder_heuristics::should_generate_builder(&type_name, obj);
        let has_builder = decision.has_builder;

        let (fields, default_fns) =
            self.generate_object_fields(nsid, &type_name, obj, has_builder, resolved)?;
        let doc = self.generate_doc_comment(obj.description.as_ref());

        // Determine Default strategy:
        // 1. Manual impl if schema defaults cover all required fields.
        // 2. derive(Default) if heuristic says so (0 required, or all-string required).
        // 3. No Default otherwise.
        let manual_default = self.generate_manual_default(nsid, &type_name, obj, resolved);
        let use_derive_default = manual_default.is_none() && decision.has_default;

        let bosstr_path = resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
        let default_str_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
        let bosstr_serde = resolved.serde_external_path(&super::prettify::ExternalImport::BosStr);
        let de_serde = resolved.serde_external_path(&super::prettify::ExternalImport::Deserialize);
        let serde_de_bound = format!("S: {}<'de> + {}", de_serde, bosstr_serde);
        let derive_attr = if use_derive_default {
            resolved.derive_standard_with(quote! { Default })
        } else {
            resolved.derive_standard()
        };

        // Generate the extra_data field directly instead of using #[lexicon] attribute.
        let smolstr_type = resolved.type_tokens(&super::prettify::CommonType::SmolStr);
        let data_type = resolved.type_tokens(&super::prettify::CommonType::Data);
        let btree_map = resolved.btree_map_path();
        let is_none_path = resolved.option_is_none_path();
        let extra_data_type =
            resolved.option_type(quote! { #btree_map<#smolstr_type, #data_type> });
        let deserialize_extra_data_name =
            format!("deserialize_{}_extra_data", type_name.to_snake_case());
        let deserialize_extra_data =
            syn::Ident::new(&deserialize_extra_data_name, proc_macro2::Span::call_site());
        let struct_def = quote! {
            #doc
            #derive_attr
            #[serde(rename_all = "camelCase", bound(deserialize = #serde_de_bound))]
            pub struct #ident<S: #bosstr_path = #default_str_path> {
                #fields
                #[serde(
                    flatten,
                    default,
                    deserialize_with = #deserialize_extra_data_name,
                    skip_serializing_if = #is_none_path
                )]
                pub extra_data: #extra_data_type,
            }
        };
        let extra_data_deserializer = quote! {
            fn #deserialize_extra_data<'de, S, D>(
                deserializer: D,
            ) -> Result<#extra_data_type, D::Error>
            where
                S: #bosstr_path + serde::Deserialize<'de>,
                D: serde::Deserializer<'de>,
            {
                let data = <#extra_data_type as serde::Deserialize<'de>>::deserialize(deserializer)?;
                Ok(data.filter(|extra_data| !extra_data.is_empty()))
            }
        };

        // Generate custom builder if needed.
        let builder = if has_builder {
            let ctx = super::builder_gen::BuilderGenContext::from_object(
                self, nsid, &type_name, obj, true, // objects always have type param
                resolved,
            );
            ctx.generate()
        } else {
            quote! {}
        };

        // Generate union types and nested object types for this object.
        let nested_items =
            self.generate_nested_types(nsid, &type_name, &obj.properties, true, resolved)?;

        // Merge nested type buckets into parent buckets.
        let mut nested_type_defs = TokenStream::new();
        let mut nested_internals = TokenStream::new();
        for nested in nested_items {
            nested_type_defs.extend(nested.type_defs);
            nested_internals.extend(nested.inherent_impls);
            nested_internals.extend(nested.trait_impls);
            nested_internals.extend(nested.internals);
        }

        // Generate LexiconSchema impl with shared lexicon_doc function.
        let (shared_fn, schema_impl) =
            self.generate_schema_impl_with_shared(&type_name, nsid, def_name, true, resolved);

        // Categorize tokens into buckets.
        let type_defs = quote! {
            #struct_def
            #nested_type_defs
        };

        let trait_impls = quote! {
            #schema_impl
        };

        let internals = quote! {
            #extra_data_deserializer
            #(#default_fns)*
            #manual_default
            #nested_internals
            #builder
            #shared_fn
        };

        Ok(GeneratedCode {
            type_defs,
            inherent_impls: TokenStream::new(),
            trait_impls,
            internals,
            imports: Default::default(),
        })
    }

    /// Generate fields for an object.
    /// Returns (field tokens, companion default functions).
    pub(super) fn generate_object_fields(
        &self,
        nsid: &str,
        parent_type_name: &str,
        obj: &LexObject<'static>,
        _is_builder: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<(TokenStream, Vec<TokenStream>)> {
        let required = obj.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
        let nullable = obj.nullable.as_ref().map(|n| n.as_slice()).unwrap_or(&[]);

        let mut fields = Vec::new();
        let mut default_fns = Vec::new();
        for (field_name, field_type) in &obj.properties {
            let is_required = required.contains(field_name);
            let is_nullable = nullable.contains(field_name);
            let (field_tokens, default_fn) = self.generate_field(
                nsid,
                parent_type_name,
                field_name,
                field_type,
                is_required,
                is_nullable,
                resolved,
            )?;
            fields.push(field_tokens);
            if let Some(f) = default_fn {
                default_fns.push(f);
            }
        }

        Ok((quote! { #(#fields)* }, default_fns))
    }

    /// Generate a single field.
    /// Returns (field tokens, optional companion default function).
    pub(super) fn generate_field(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        field_type: &LexObjectProperty<'static>,
        is_required: bool,
        is_nullable: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<(TokenStream, Option<TokenStream>)> {
        if field_name.is_empty() {
            eprintln!(
                "Warning: Empty field name in lexicon '{}' type '{}', using 'unknown' as fallback",
                nsid, parent_type_name
            );
        }
        let field_ident = make_ident(&field_name.to_snake_case());

        let rust_type =
            self.property_to_rust_type(nsid, parent_type_name, field_name, field_type, resolved)?;

        let is_optional = !is_required || is_nullable;
        let rust_type = if !is_optional {
            rust_type
        } else {
            resolved.option_type(rust_type)
        };

        // Extract description from field type.
        let description = match field_type {
            LexObjectProperty::Ref(r) => r.description.as_ref(),
            LexObjectProperty::Union(u) => u.description.as_ref(),
            LexObjectProperty::Bytes(b) => b.description.as_ref(),
            LexObjectProperty::CidLink(c) => c.description.as_ref(),
            LexObjectProperty::Array(a) => a.description.as_ref(),
            LexObjectProperty::Blob(b) => b.description.as_ref(),
            LexObjectProperty::Object(o) => o.description.as_ref(),
            LexObjectProperty::Boolean(b) => b.description.as_ref(),
            LexObjectProperty::Integer(i) => i.description.as_ref(),
            LexObjectProperty::String(s) => s.description.as_ref(),
            LexObjectProperty::Unknown(u) => u.description.as_ref(),
        };

        // Extract schema default and generate companion function + serde attr.
        let (default_doc, serde_default_attr, default_fn) = self.extract_field_default(
            nsid,
            parent_type_name,
            field_name,
            field_type,
            is_optional,
            resolved,
        );

        // Combine description with default doc suffix.
        let combined_desc = match (description, &default_doc) {
            (Some(desc), Some(def_doc)) => Some(format!("{} {}", desc.as_ref(), def_doc)),
            (Some(desc), None) => Some(desc.as_ref().to_string()),
            (None, Some(def_doc)) => Some(def_doc.clone()),
            (None, None) => None,
        };
        let doc = combined_desc
            .as_ref()
            .map(|d| generate_doc_comment(Some(&CowStr::copy_from_str(d))))
            .unwrap_or_default();

        let mut attrs = Vec::new();

        if is_optional {
            let is_none_path = resolved.option_is_none_path();
            attrs.push(quote! { #[serde(skip_serializing_if = #is_none_path)] });
        }

        if let Some(serde_attr) = serde_default_attr {
            attrs.push(serde_attr);
        }

        if matches!(field_type, LexObjectProperty::Bytes(_)) {
            if !is_optional {
                attrs.push(quote! { #[serde(with = "jacquard_common::serde_bytes_helper")] });
            } else {
                attrs.push(
                    quote! {#[serde(default, with = "jacquard_common::opt_serde_bytes_helper")] },
                );
            }
        }

        Ok((
            quote! {
                #doc
                #(#attrs)*
                pub #field_ident: #rust_type,
            },
            default_fn,
        ))
    }

    /// Extract schema default value from a field type and generate the companion
    /// default function and serde attribute.
    ///
    /// Returns (doc_suffix, serde_attr, companion_fn).
    fn extract_field_default(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        field_type: &LexObjectProperty<'static>,
        is_optional: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> (Option<String>, Option<TokenStream>, Option<TokenStream>) {
        let fn_name = format!(
            "_default_{}_{}",
            parent_type_name.to_snake_case(),
            field_name.to_snake_case()
        );
        let fn_ident = syn::Ident::new(&fn_name, proc_macro2::Span::call_site());
        let serde_attr = quote! { #[serde(default = #fn_name)] };

        match field_type {
            LexObjectProperty::Boolean(b) if b.default.is_some() => {
                let v = b.default.unwrap();
                let doc = format!(" Defaults to `{}`.", v);
                if is_optional {
                    let opt_bool = resolved.option_type(quote! { bool });
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> #opt_bool { Some(#v) }
                        }),
                    )
                } else {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> bool { #v }
                        }),
                    )
                }
            }
            LexObjectProperty::Integer(i) if i.default.is_some() => {
                let v = i.default.unwrap();
                let doc = format!(" Defaults to `{}`.", v);
                if is_optional {
                    let opt_i64 = resolved.option_type(quote! { i64 });
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> #opt_i64 { Some(#v) }
                        }),
                    )
                } else {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> i64 { #v }
                        }),
                    )
                }
            }
            LexObjectProperty::String(s) if s.default.is_some() => {
                let v = s.default.as_ref().unwrap().as_ref();
                let doc = format!(" Defaults to `\"{}\"`.", v);
                // The default function is generic over S: FromStaticStr.
                // Type inference picks up S from the field's expected type,
                // and the serde(bound) on the struct ensures the bound is met.
                let from_static_path =
                    resolved.external_type_tokens(&super::prettify::ExternalImport::FromStaticStr);
                if s.known_values.is_some() || s.r#enum.is_some() {
                    // Enum-constrained field: the generated enum type has
                    // this default as a known variant. The qualified-angle
                    // form avoids syn misparsing the path in attribute
                    // position.
                    let enum_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    let enum_ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());
                    let bosstr_path =
                        resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
                    if is_optional {
                        (
                            Some(doc),
                            Some(serde_attr),
                            Some(quote! {
                                fn #fn_ident<S: #from_static_path + #bosstr_path>() -> ::core::option::Option<#enum_ident<S>> {
                                    Some(<#enum_ident<S>>::from_value(S::from_static(#v)))
                                }
                            }),
                        )
                    } else {
                        (
                            Some(doc),
                            Some(serde_attr),
                            Some(quote! {
                                fn #fn_ident<S: #from_static_path + #bosstr_path>() -> #enum_ident<S> {
                                    <#enum_ident<S>>::from_value(S::from_static(#v))
                                }
                            }),
                        )
                    }
                } else if is_optional {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident<S: #from_static_path>() -> ::core::option::Option<S> {
                                Some(S::from_static(#v))
                            }
                        }),
                    )
                } else {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident<S: #from_static_path>() -> S {
                                S::from_static(#v)
                            }
                        }),
                    )
                }
            }
            _ => (None, None, None),
        }
    }

    /// Generate a manual `impl Default` for a struct when all required fields have
    /// schema defaults. Optional fields default to `None` or `Some(schema_default)`.
    pub(super) fn generate_manual_default(
        &self,
        nsid: &str,
        type_name: &str,
        obj: &LexObject<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Option<TokenStream> {
        if !super::builder_heuristics::eligible_for_schema_default(obj) {
            return None;
        }

        // Check if any field actually has a schema default. If none do,
        // the existing derive(Default) is sufficient.
        let any_schema_default = obj
            .properties
            .values()
            .any(|p| super::builder_heuristics::has_schema_default(p));
        if !any_schema_default {
            return None;
        }

        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());
        let required = obj.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
        let nullable = obj.nullable.as_ref().map(|n| n.as_slice()).unwrap_or(&[]);

        let field_defaults: Vec<_> = obj
            .properties
            .iter()
            .map(|(field_name, field_type)| {
                let field_ident = make_ident(&field_name.to_snake_case());
                let is_required = required.contains(field_name);
                let is_nullable = nullable.contains(field_name);
                let is_optional = !is_required || is_nullable;

                let value = self.schema_default_value(
                    nsid,
                    type_name,
                    field_name,
                    field_type,
                    is_optional,
                    resolved,
                );
                quote! { #field_ident: #value }
            })
            .collect();

        // Manual Default impl uses DefaultStr (SmolStr) since string fields
        // with schema defaults need a concrete string type for construction.
        Some(quote! {
            impl Default for #ident {
                fn default() -> Self {
                    Self {
                        #(#field_defaults,)*
                        extra_data: Default::default(),
                    }
                }
            }
        })
    }

    /// Generate the default value expression for a field.
    fn schema_default_value(
        &self,
        nsid: &str,
        type_name: &str,
        field_name: &str,
        field_type: &LexObjectProperty<'static>,
        is_optional: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> TokenStream {
        let inner = match field_type {
            LexObjectProperty::Boolean(b) if b.default.is_some() => {
                let v = b.default.unwrap();
                Some(quote! { #v })
            }
            LexObjectProperty::Integer(i) if i.default.is_some() => {
                let v = i.default.unwrap();
                Some(quote! { #v })
            }
            LexObjectProperty::String(s) if s.default.is_some() => {
                let v = s.default.as_ref().unwrap().as_ref();
                if s.known_values.is_some() || s.r#enum.is_some() {
                    // Enum field: DefaultStr-backed enum via from_value.
                    let enum_name = self.generate_field_type_name(nsid, type_name, field_name, "");
                    let enum_ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());
                    Some(quote! {
                        <#enum_ident>::from_value(jacquard_common :: DefaultStr :: from_static(#v))
                    })
                } else {
                    let smolstr_path = resolved.type_path(&super::prettify::CommonType::SmolStr);
                    Some(quote! { #smolstr_path::from(#v) })
                }
            }
            _ => None,
        };

        match (inner, is_optional) {
            (Some(val), true) => quote! { Some(#val) },
            (Some(val), false) => val,
            (None, true) => quote! { None },
            (None, false) => quote! { Default::default() },
        }
    }

    /// Generate a union enum for refs
    pub fn generate_union(
        &self,
        current_nsid: &str,
        union_name: &str,
        refs: &[jacquard_common::CowStr<'static>],
        description: Option<&str>,
        closed: Option<bool>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let enum_ident = syn::Ident::new(union_name, proc_macro2::Span::call_site());

        // Build variants using the union_codegen module
        let ctx = super::union_codegen::UnionGenContext {
            corpus: self.corpus,
            namespace_deps: &self.namespace_deps,
            current_nsid,
        };

        let union_variants =
            ctx.build_union_variants(refs, |ref_str| self.ref_to_rust_type(ref_str, resolved))?;
        let variants = super::union_codegen::generate_variant_tokens(&union_variants);

        let doc = description
            .map(|d| quote! { #[doc = #d] })
            .unwrap_or_else(|| quote! {});

        // Only add open_union if not closed.
        let is_open = closed != Some(true);
        let derive_attr = resolved.derive_standard();
        let bosstr_path = resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
        let default_str_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
        let bosstr_serde = resolved.serde_external_path(&super::prettify::ExternalImport::BosStr);
        let de_serde = resolved.serde_external_path(&super::prettify::ExternalImport::Deserialize);
        let serde_de_bound = format!("S: {}<'de> + {}", de_serde, bosstr_serde);

        let serde_bound = quote! {
            #[serde(tag = "$type", bound(deserialize = #serde_de_bound))]
        };

        let enum_def = if is_open {
            let open_union_attr =
                resolved.attribute_tokens(&super::prettify::ExternalImport::OpenUnion);
            quote! {
                #doc
                #open_union_attr
                #derive_attr
                #serde_bound
                pub enum #enum_ident<S: #bosstr_path = #default_str_path> {
                    #(#variants,)*
                }
            }
        } else {
            quote! {
                #doc
                #derive_attr
                #serde_bound
                pub enum #enum_ident<S: #bosstr_path = #default_str_path> {
                    #(#variants,)*
                }
            }
        };

        Ok(GeneratedCode::type_only(enum_def))
    }

    /// Generate a permissioned-space declaration marker.
    ///
    /// `type: "space"` is lexicon metadata rather than a wire object. Keep the
    /// declaration available to repository code without inventing a serde
    /// representation for fields that are never sent on the wire.
    pub(super) fn generate_space(
        &self,
        nsid: &str,
        def_name: &str,
        space: &crate::lexicon::LexSpace<'static>,
    ) -> Result<GeneratedCode> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
        let doc = self.generate_doc_comment(space.description.as_ref());
        let key = space.key.as_ref().map(|value| value.as_ref()).unwrap_or("");
        let name = space
            .name
            .as_ref()
            .map(|value| value.as_ref())
            .unwrap_or("");
        let collections: Vec<&str> = space
            .collections
            .iter()
            .map(|value| value.as_ref())
            .collect();

        let tokens = quote! {
            #doc
            #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
            pub struct #ident;

            impl #ident {
                pub const KEY: &'static str = #key;
                pub const NAME: &'static str = #name;
                pub const COLLECTIONS: &'static [&'static str] = &[#(#collections),*];
            }
        };
        Ok(GeneratedCode::type_only(tokens))
    }

    /// Generate enum for string with known values.
    pub(super) fn generate_known_values_enum(
        &self,
        nsid: &str,
        def_name: &str,
        string: &LexString<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        let known_values = string.known_values.as_ref().unwrap();
        let mut variants = Vec::new();
        let mut from_str_arms = Vec::new();
        let mut as_str_arms = Vec::new();

        let mut known_variant_names = std::collections::HashSet::new();
        for value in known_values {
            // Convert value to valid Rust identifier
            let value_str = value.as_ref();
            let variant_name = value_to_variant_name(value_str);
            known_variant_names.insert(variant_name.clone());
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            variants.push(quote! {
                #variant_ident
            });

            from_str_arms.push(quote! {
                #value_str => Self::#variant_ident
            });

            as_str_arms.push(quote! {
                Self::#variant_ident => #value_str
            });
        }

        // Choose catch-all name, falling back if "Other" collides with a known value variant.
        let catchall_name = if known_variant_names.contains("Other") {
            "UnknownValue"
        } else {
            "Other"
        };
        let catchall_ident = syn::Ident::new(catchall_name, proc_macro2::Span::call_site());

        let doc = self.generate_doc_comment(string.description.as_ref());

        // Generate IntoStatic impl
        let variant_info: Vec<(String, EnumVariantKind)> = known_values
            .iter()
            .map(|value| {
                let variant_name = value_to_variant_name(value.as_ref());
                (variant_name, EnumVariantKind::Unit)
            })
            .chain(std::iter::once((
                catchall_name.to_string(),
                EnumVariantKind::Tuple,
            )))
            .collect();
        let into_static_impl =
            self.generate_into_static_for_enum(&type_name, &variant_info, true, false, resolved);

        let cowstr_type = resolved.type_tokens(&super::prettify::CommonType::CowStr);
        let bosstr_path = resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
        let default_str_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
        let ser_serde = resolved.external_type_tokens(&super::prettify::ExternalImport::Serialize);
        let de_serde = resolved.external_type_tokens(&super::prettify::ExternalImport::Deserialize);
        let enum_def = quote! {
            #doc
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #ident<S: #bosstr_path = #default_str_path> {
                #(#variants,)*
                #catchall_ident(#cowstr_type),
            }

            impl<S: #bosstr_path> #ident<S> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms,)*
                        Self::#catchall_ident(s) => s.as_ref(),
                    }
                }

                /// Construct from a string-like value, matching known values.
                pub fn from_value(s: S) -> Self {
                    match s.as_ref() {
                        #(#from_str_arms,)*
                        _ => Self::#catchall_ident(s),
                    }
                }
            }

            impl<S: #bosstr_path> AsRef<str> for #ident<S> {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }

            impl<S: #bosstr_path> core::fmt::Display for #ident<S> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}", self.as_str())
                }
            }

            impl<S: #bosstr_path> #ser_serde for #ident<S> {
                fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
                where
                    Ser: serde::Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }

            impl<'de, S: #de_serde<'de> + #bosstr_path> #de_serde<'de> for #ident<S> {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let s = S::deserialize(deserializer)?;
                    Ok(Self::from_value(s))
                }
            }

            #into_static_impl
        };

        Ok(GeneratedCode::type_only(enum_def))
    }

    /// Generate enum for inline string property with known values.
    /// Unlike `generate_known_values_enum`, this takes the type name directly
    /// and uses fragment extraction for NSID#fragment values.
    /// Generate an inline string enum from `enum` or `knownValues`
    /// constraints (the two are equivalent for generation purposes).
    ///
    /// Falls back to a plain string type with associated value constants
    /// when the values cannot be named as Rust variants (emoji,
    /// punctuation-only) or would collide after naming.
    pub(super) fn generate_inline_string_enum(
        &self,
        type_name: &str,
        string: &LexString<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        if !string_enum_is_nameable(string) {
            let values: Vec<CowStr<'static>> = string
                .known_values
                .clone()
                .or_else(|| string.r#enum.clone())
                .unwrap_or_default();
            return self.generate_string_constants(type_name, string, &values);
        }
        let mut string = string.clone();
        if string.known_values.is_none() {
            string.known_values = string.r#enum.clone();
        }
        self.generate_inline_known_values_enum(type_name, &string, resolved)
    }

    /// Fallback for unnameable enum values: keep the field a plain string
    /// type and emit the constrained values as associated constants.
    fn generate_string_constants(
        &self,
        type_name: &str,
        string: &LexString<'static>,
        values: &[CowStr<'static>],
    ) -> Result<GeneratedCode> {
        use heck::ToShoutySnakeCase;

        let doc = self.generate_doc_comment(string.description.as_ref());
        // One nameable sibling (or the field name) anchors the module;
        // constants are named from any nameable prefix of each value,
        // else positional.
        let mut consts = Vec::new();
        for (i, v) in values.iter().enumerate() {
            let base = known_value_to_variant_name(v.as_ref());
            let name = if base.is_empty() || base == "Unknown" {
                format!("Value{}", i + 1)
            } else {
                base
            };
            let const_ident =
                syn::Ident::new(&name.to_shouty_snake_case(), proc_macro2::Span::call_site());
            let value_str = v.as_ref();
            consts.push(quote! {
                #doc
                pub const #const_ident: &'static str = #value_str;
            });
        }

        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());
        let type_def = quote! {
            /// Constrained string values.
            ///
            /// The lexicon restricts this string to the constants below;
            /// the type stays a plain string because the values are not
            /// representable as Rust variants.
            pub struct #ident;

            impl #ident {
                #(#consts)*
            }
        };

        Ok(GeneratedCode::type_only(type_def))
    }

    pub(super) fn generate_inline_known_values_enum(
        &self,
        type_name: &str,
        string: &LexString<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

        let known_values = string.known_values.as_ref().unwrap();
        let mut variants = Vec::new();
        let mut from_str_arms = Vec::new();
        let mut as_str_arms = Vec::new();
        let mut known_variant_names = std::collections::HashSet::new();

        for value in known_values {
            let value_str = value.as_ref();
            // Use known_value_to_variant_name to extract fragment from NSID#fragment
            let variant_name = known_value_to_variant_name(value_str);
            known_variant_names.insert(variant_name.clone());
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            variants.push(quote! {
                #variant_ident
            });

            from_str_arms.push(quote! {
                #value_str => Self::#variant_ident
            });

            as_str_arms.push(quote! {
                Self::#variant_ident => #value_str
            });
        }

        // Choose catch-all name, falling back if "Other" collides with a known value variant.
        let catchall_name = if known_variant_names.contains("Other") {
            "UnknownValue"
        } else {
            "Other"
        };
        let catchall_ident = syn::Ident::new(catchall_name, proc_macro2::Span::call_site());

        let doc = self.generate_doc_comment(string.description.as_ref());

        // Generate IntoStatic impl
        let variant_info: Vec<(String, EnumVariantKind)> = known_values
            .iter()
            .map(|value| {
                let variant_name = known_value_to_variant_name(value.as_ref());
                (variant_name, EnumVariantKind::Unit)
            })
            .chain(std::iter::once((
                catchall_name.to_string(),
                EnumVariantKind::Tuple,
            )))
            .collect();
        let into_static_impl =
            self.generate_into_static_for_enum(type_name, &variant_info, true, false, resolved);

        let cowstr_type = resolved.type_tokens(&super::prettify::CommonType::CowStr);
        let bosstr_path = resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);
        let default_str_path =
            resolved.external_type_tokens(&super::prettify::ExternalImport::DefaultStr);
        let ser_serde = resolved.external_type_tokens(&super::prettify::ExternalImport::Serialize);
        let de_serde = resolved.external_type_tokens(&super::prettify::ExternalImport::Deserialize);
        let enum_def = quote! {
            #doc
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #ident<S: #bosstr_path = #default_str_path> {
                #(#variants,)*
                #catchall_ident(#cowstr_type),
            }

            impl<S: #bosstr_path> #ident<S> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms,)*
                        Self::#catchall_ident(s) => s.as_ref(),
                    }
                }

                /// Construct from a string-like value, matching known values.
                pub fn from_value(s: S) -> Self {
                    match s.as_ref() {
                        #(#from_str_arms,)*
                        _ => Self::#catchall_ident(s),
                    }
                }
            }

            impl<S: #bosstr_path> core::fmt::Display for #ident<S> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}", self.as_str())
                }
            }

            impl<S: #bosstr_path> AsRef<str> for #ident<S> {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }

            impl<S: #bosstr_path> #ser_serde for #ident<S> {
                fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
                where
                    Ser: serde::Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }

            impl<'de, S: #de_serde<'de> + #bosstr_path> #de_serde<'de> for #ident<S> {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let s = S::deserialize(deserializer)?;
                    Ok(Self::from_value(s))
                }
            }

            impl<S: #bosstr_path + Default> Default for #ident<S> {
                fn default() -> Self {
                    Self::#catchall_ident(Default::default())
                }
            }

            #into_static_impl
        };

        Ok(GeneratedCode::type_only(enum_def))
    }

    /// Generate enum for integer with enum values
    pub(super) fn generate_integer_enum(
        &self,
        nsid: &str,
        def_name: &str,
        integer: &LexInteger<'static>,
    ) -> Result<GeneratedCode> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        let enum_values = integer.r#enum.as_ref().unwrap();
        let mut variants = Vec::new();
        let mut from_i64_arms = Vec::new();
        let mut to_i64_arms = Vec::new();

        for value in enum_values {
            let variant_name = format!("Value{}", value.abs());
            let variant_ident = syn::Ident::new(&variant_name, proc_macro2::Span::call_site());

            variants.push(quote! {
                #[serde(rename = #value)]
                #variant_ident
            });

            from_i64_arms.push(quote! {
                #value => Self::#variant_ident
            });

            to_i64_arms.push(quote! {
                Self::#variant_ident => #value
            });
        }

        let doc = self.generate_doc_comment(integer.description.as_ref());

        let enum_def = quote! {
            #doc
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum #ident {
                #(#variants,)*
                #[serde(untagged)]
                Other(i64),
            }

            impl #ident {
                pub fn as_i64(&self) -> i64 {
                    match self {
                        #(#to_i64_arms,)*
                        Self::Other(n) => *n,
                    }
                }
            }

            impl From<i64> for #ident {
                fn from(n: i64) -> Self {
                    match n {
                        #(#from_i64_arms,)*
                        _ => Self::Other(n),
                    }
                }
            }

            impl serde::Serialize for #ident {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serializer.serialize_i64(self.as_i64())
                }
            }

            impl<'de> serde::Deserialize<'de> for #ident {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let n = i64::deserialize(deserializer)?;
                    Ok(Self::from(n))
                }
            }
        };

        Ok(GeneratedCode::type_only(enum_def))
    }

    /// Generate IntoStatic impl for an enum.
    pub(super) fn generate_into_static_for_enum(
        &self,
        type_name: &str,
        variant_info: &[(String, EnumVariantKind)],
        has_type_param: bool,
        is_open: bool,
        resolved: &super::prettify::ResolvedImports,
    ) -> TokenStream {
        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());
        let bosstr_path = resolved.external_type_tokens(&super::prettify::ExternalImport::BosStr);

        if has_type_param {
            let variant_conversions: Vec<_> = variant_info
                .iter()
                .map(|(variant_name, kind)| {
                    let variant_ident = syn::Ident::new(variant_name, proc_macro2::Span::call_site());
                    match kind {
                        EnumVariantKind::Unit => {
                            quote! {
                                #ident::#variant_ident => #ident::#variant_ident
                            }
                        }
                        EnumVariantKind::Tuple => {
                            quote! {
                                #ident::#variant_ident(v) => #ident::#variant_ident(v.into_static())
                            }
                        }
                        EnumVariantKind::Struct(fields) => {
                            let field_idents: Vec<_> = fields
                                .iter()
                                .map(|f| make_ident(&f.to_snake_case()))
                                .collect();
                            let field_conversions: Vec<_> = field_idents
                                .iter()
                                .map(|f| quote! { #f: #f.into_static() })
                                .collect();
                            quote! {
                                #ident::#variant_ident { #(#field_idents,)* } => #ident::#variant_ident {
                                    #(#field_conversions,)*
                                }
                            }
                        }
                    }
                })
                .collect();

            let unknown_conversion = if is_open {
                quote! {
                    #ident::Unknown(v) => #ident::Unknown(v.into_static()),
                }
            } else {
                quote! {}
            };

            quote! {
                impl<S: #bosstr_path> jacquard_common::IntoStatic for #ident<S>
                where
                    S: #bosstr_path + jacquard_common::IntoStatic,
                    S::Output: #bosstr_path
                {
                    type Output = #ident<S::Output>;

                    fn into_static(self) -> Self::Output {
                        match self {
                            #(#variant_conversions,)*
                            #unknown_conversion
                        }
                    }
                }
            }
        } else {
            quote! {
                impl jacquard_common::IntoStatic for #ident {
                    type Output = #ident;

                    fn into_static(self) -> Self::Output {
                        self
                    }
                }
            }
        }
    }
}
