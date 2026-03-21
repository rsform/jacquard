use crate::error::Result;
use crate::lexicon::{
    LexArrayItem, LexInteger, LexObject, LexObjectProperty, LexRecord, LexString,
};
use heck::ToSnakeCase;
use jacquard_common::deps::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeMap;

use super::CodeGenerator;
use super::utils::{known_value_to_variant_name, make_ident, value_to_variant_name};
use super::prettify::GeneratedCode;

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
                        nested.push(
                            self.generate_union(nsid, &union_name, &refs, None, union.closed, resolved)?,
                        );
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
                            nested.push(
                                self.generate_union(nsid, &union_name, &refs, None, union.closed, resolved)?,
                            );
                        }
                    }
                }
                LexObjectProperty::String(s) if s.known_values.is_some() => {
                    let enum_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    nested.push(self.generate_inline_known_values_enum(&enum_name, s, resolved)?);
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

                // Records always get a lifetime since they have the #[lexicon] attribute
                // which adds extra_data: BTreeMap<..., Data<'a>>
                // Skip custom builder for types that conflict with the macro's unqualified type references
                let has_builder =
                    !super::builder_heuristics::conflicts_with_builder_macro(&type_name);

                // Generate main struct fields.
                let (fields, default_fns) =
                    self.generate_object_fields(nsid, &type_name, obj, has_builder, resolved)?;
                let doc = self.generate_doc_comment(record.description.as_ref());
                let manual_default = self.generate_manual_default(&type_name, obj, resolved);

                let derive_attr = resolved.derive_standard();
                let lexicon_attr = resolved.attribute_tokens(&super::prettify::ExternalImport::LexiconAttr);
                let struct_def = quote! {
                    #doc
                    #lexicon_attr
                    #derive_attr
                    #[serde(rename_all = "camelCase")]
                    pub struct #ident<'a> {
                        #fields
                    }
                };

                // Generate custom builder if needed
                let builder = if has_builder {
                    let ctx = super::builder_gen::BuilderGenContext::from_object(
                        self, nsid, &type_name, obj, true, // records always have lifetime
                    );
                    ctx.generate()
                } else {
                    quote! {}
                };

                // Generate union types and nested object types for this record
                let unions = self.generate_nested_types(nsid, &type_name, &obj.properties, true, resolved)?;

                // Generate typed GetRecordOutput wrapper
                let output_type_name = format!("{}GetRecordOutput", type_name);
                let output_type_ident =
                    syn::Ident::new(&output_type_name, proc_macro2::Span::call_site());

                let is_none_path = resolved.option_is_none_path();
                let cid_type = resolved.type_tokens(&super::prettify::CommonType::Cid);
                let at_uri_type = resolved.type_tokens(&super::prettify::CommonType::AtUri);
                let output_wrapper = quote! {
                    /// Typed wrapper for GetRecord response with this collection's record type.
                    #derive_attr
                    #[serde(rename_all = "camelCase")]
                    pub struct #output_type_ident<'a> {
                        #[serde(skip_serializing_if = #is_none_path)]
                        #[serde(borrow)]
                        pub cid: core::option::Option<#cid_type>,
                        #[serde(borrow)]
                        pub uri: #at_uri_type,
                        #[serde(borrow)]
                        pub value: #ident<'a>,
                    }
                };

                // Generate marker struct for XrpcResp.
                let record_marker_name = format!("{}Record", type_name);
                let record_marker_ident =
                    syn::Ident::new(&record_marker_name, proc_macro2::Span::call_site());

                let ser_path = resolved.external_type_tokens(&super::prettify::ExternalImport::Serialize);
                let de_path = resolved.external_type_tokens(&super::prettify::ExternalImport::Deserialize);
                let record_marker = quote! {
                    /// Marker type for deserializing records from this collection.
                    #[derive(Debug, #ser_path, #de_path)]
                    pub struct #record_marker_ident;

                    impl jacquard_common::xrpc::XrpcResp for #record_marker_ident {
                        const NSID: &'static str = #nsid;
                        const ENCODING: &'static str = "application/json";
                        type Output<'de> = #output_type_ident<'de>;
                        type Err<'de> = jacquard_common::types::collection::RecordError<'de>;
                    }


                };
                let from_impl = quote! {
                    impl From<#output_type_ident<'_>> for #ident<'_> {
                        fn from(output: #output_type_ident<'_>) -> Self {
                            use jacquard_common::IntoStatic;
                            output.value.into_static()
                        }
                    }
                };

                // Generate Collection trait impl.
                let collection_impl = quote! {
                    impl jacquard_common::types::collection::Collection for #ident<'_> {
                        const NSID: &'static str = #nsid;
                        type Record = #record_marker_ident;
                    }
                };

                // Generate collection impl for the marker struct to drive fetch_record().
                let collection_marker_impl = quote! {
                    impl jacquard_common::types::collection::Collection for #record_marker_ident {
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

                let cowstr_type = resolved.type_tokens(&super::prettify::CommonType::CowStr);
                let at_uri_path = resolved.type_path(&super::prettify::CommonType::AtUri);
                let inherent_impls = quote! {
                    impl<'a> #ident<'a> {
                        pub fn uri(uri: impl Into<#cowstr_type>) -> Result<jacquard_common::types::uri::RecordUri<'a, #record_marker_ident>, jacquard_common::types::uri::UriError> {
                            jacquard_common::types::uri::RecordUri::try_from_uri(#at_uri_path::new_cow(uri.into())?)
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

        // Objects always get a lifetime since they have the #[lexicon] attribute
        // which adds extra_data: BTreeMap<..., Data<'a>>

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
        let manual_default = self.generate_manual_default(&type_name, obj, resolved);
        let use_derive_default = manual_default.is_none() && decision.has_default;

        let lexicon_attr = resolved.attribute_tokens(&super::prettify::ExternalImport::LexiconAttr);
        let derive_attr = if use_derive_default {
            resolved.derive_standard_with(quote! { Default })
        } else {
            resolved.derive_standard()
        };
        let struct_def = quote! {
            #doc
            #lexicon_attr
            #derive_attr
            #[serde(rename_all = "camelCase")]
            pub struct #ident<'a> {
                #fields
            }
        };

        // Generate custom builder if needed
        let builder = if has_builder {
            let ctx = super::builder_gen::BuilderGenContext::from_object(
                self, nsid, &type_name, obj, true, // objects always have lifetime
            );
            ctx.generate()
        } else {
            quote! {}
        };

        // Generate union types and nested object types for this object.
        let nested_items = self.generate_nested_types(nsid, &type_name, &obj.properties, true, resolved)?;

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
        let needs_lifetime = self.property_needs_lifetime(field_type);

        let is_optional = !is_required || is_nullable;
        let rust_type = if !is_optional {
            rust_type
        } else {
            quote! { core::option::Option<#rust_type> }
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
        let (default_doc, serde_default_attr, default_fn) =
            self.extract_field_default(parent_type_name, field_name, field_type, is_optional, resolved);

        // Combine description with default doc suffix.
        let combined_desc = match (description, &default_doc) {
            (Some(desc), Some(def_doc)) => {
                Some(format!("{} {}", desc.as_ref(), def_doc))
            }
            (Some(desc), None) => Some(desc.as_ref().to_string()),
            (None, Some(def_doc)) => Some(def_doc.clone()),
            (None, None) => None,
        };
        let doc = combined_desc
            .as_ref()
            .map(|d| {
                let d = d.as_str();
                quote! { #[doc = #d] }
            })
            .unwrap_or_default();

        let mut attrs = Vec::new();

        if is_optional {
            let is_none_path = resolved.option_is_none_path();
            attrs.push(quote! { #[serde(skip_serializing_if = #is_none_path)] });
        }

        if let Some(serde_attr) = serde_default_attr {
            attrs.push(serde_attr);
        }

        // Add serde(borrow) to all fields with lifetimes.
        if needs_lifetime {
            attrs.push(quote! { #[serde(borrow)] });
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
                let doc = format!("Defaults to `{}`.", v);
                if is_optional {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> core::option::Option<bool> { Some(#v) }
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
                let doc = format!("Defaults to `{}`.", v);
                if is_optional {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> core::option::Option<i64> { Some(#v) }
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
            LexObjectProperty::String(s) if s.default.is_some() && s.known_values.is_none() => {
                let v = s.default.as_ref().unwrap().as_ref();
                let doc = format!("Defaults to `\"{}\"`.", v);
                let cowstr_path = resolved.type_path(&super::prettify::CommonType::CowStr);
                if is_optional {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> core::option::Option<#cowstr_path<'static>> {
                                Some(#cowstr_path::from(#v))
                            }
                        }),
                    )
                } else {
                    (
                        Some(doc),
                        Some(serde_attr),
                        Some(quote! {
                            fn #fn_ident() -> #cowstr_path<'static> {
                                #cowstr_path::from(#v)
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
        type_name: &str,
        obj: &LexObject<'static>,
        resolved: &super::prettify::ResolvedImports,
    ) -> Option<TokenStream> {
        if !super::builder_heuristics::eligible_for_schema_default(obj) {
            return None;
        }

        // Check if any field actually has a schema default. If none do,
        // the existing derive(Default) is sufficient.
        let any_schema_default = obj.properties.values().any(|p| {
            super::builder_heuristics::has_schema_default(p)
        });
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

                let value = self.schema_default_value(field_type, is_optional, resolved);
                quote! { #field_ident: #value }
            })
            .collect();

        Some(quote! {
            impl Default for #ident<'_> {
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
            LexObjectProperty::String(s) if s.default.is_some() && s.known_values.is_none() => {
                let v = s.default.as_ref().unwrap().as_ref();
                let cowstr_path = resolved.type_path(&super::prettify::CommonType::CowStr);
                Some(quote! { #cowstr_path::from(#v) })
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

        let enum_def = if is_open {
            let open_union_attr = resolved.attribute_tokens(&super::prettify::ExternalImport::OpenUnion);
            quote! {
                #doc
                #open_union_attr
                #derive_attr
                #[serde(tag = "$type")]
                #[serde(bound(deserialize = "'de: 'a"))]
                pub enum #enum_ident<'a> {
                    #(#variants,)*
                }
            }
        } else {
            quote! {
                #doc
                #derive_attr
                #[serde(tag = "$type")]
                #[serde(bound(deserialize = "'de: 'a"))]
                pub enum #enum_ident<'a> {
                    #(#variants,)*
                }
            }
        };

        Ok(GeneratedCode::type_only(enum_def))
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
            self.generate_into_static_for_enum(&type_name, &variant_info, true, false);

        let cowstr_type = resolved.type_tokens(&super::prettify::CommonType::CowStr);
        let cowstr_path = resolved.type_path(&super::prettify::CommonType::CowStr);
        let enum_def = quote! {
            #doc
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #ident<'a> {
                #(#variants,)*
                #catchall_ident(#cowstr_type),
            }

            impl<'a> #ident<'a> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms,)*
                        Self::#catchall_ident(s) => s.as_ref(),
                    }
                }
            }

            impl<'a> From<&'a str> for #ident<'a> {
                fn from(s: &'a str) -> Self {
                    match s {
                        #(#from_str_arms,)*
                        _ => Self::#catchall_ident(#cowstr_path::from(s)),
                    }
                }
            }

            impl<'a> From<String> for #ident<'a> {
                fn from(s: String) -> Self {
                    match s.as_str() {
                        #(#from_str_arms,)*
                        _ => Self::#catchall_ident(#cowstr_path::from(s)),
                    }
                }
            }

            impl<'a> AsRef<str> for #ident<'a> {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }

            impl<'a> core::fmt::Display for #ident<'a> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}", self.as_str())
                }
            }

            impl<'a> serde::Serialize for #ident<'a> {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }

            impl<'de, 'a> serde::Deserialize<'de> for #ident<'a>
            where
                'de: 'a,
            {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let s = <&'de str>::deserialize(deserializer)?;
                    Ok(Self::from(s))
                }
            }

            #into_static_impl
        };

        Ok(GeneratedCode::type_only(enum_def))
    }

    /// Generate enum for inline string property with known values.
    /// Unlike `generate_known_values_enum`, this takes the type name directly
    /// and uses fragment extraction for NSID#fragment values.
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
            self.generate_into_static_for_enum(type_name, &variant_info, true, false);

        let cowstr_type = resolved.type_tokens(&super::prettify::CommonType::CowStr);
        let cowstr_path = resolved.type_path(&super::prettify::CommonType::CowStr);
        let enum_def = quote! {
            #doc
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #ident<'a> {
                #(#variants,)*
                #catchall_ident(#cowstr_type),
            }

            impl<'a> #ident<'a> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms,)*
                        Self::#catchall_ident(s) => s.as_ref(),
                    }
                }
            }

            impl<'a> From<&'a str> for #ident<'a> {
                fn from(s: &'a str) -> Self {
                    match s {
                        #(#from_str_arms,)*
                        _ => Self::#catchall_ident(#cowstr_path::from(s)),
                    }
                }
            }

            impl<'a> From<String> for #ident<'a> {
                fn from(s: String) -> Self {
                    match s.as_str() {
                        #(#from_str_arms,)*
                        _ => Self::#catchall_ident(#cowstr_path::from(s)),
                    }
                }
            }

            impl<'a> core::fmt::Display for #ident<'a> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}", self.as_str())
                }
            }

            impl<'a> AsRef<str> for #ident<'a> {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }

            impl<'a> serde::Serialize for #ident<'a> {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }

            impl<'de, 'a> serde::Deserialize<'de> for #ident<'a>
            where
                'de: 'a,
            {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let s = <&'de str>::deserialize(deserializer)?;
                    Ok(Self::from(s))
                }
            }

            impl<'a> Default for #ident<'a> {
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

    /// Generate IntoStatic impl for a struct
    #[allow(dead_code)]
    pub(super) fn generate_into_static_for_struct(
        &self,
        type_name: &str,
        field_names: &[&str],
        has_lifetime: bool,
        has_extra_data: bool,
    ) -> TokenStream {
        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

        let field_idents: Vec<_> = field_names
            .iter()
            .map(|name| make_ident(&name.to_snake_case()))
            .collect();

        if has_lifetime {
            let field_conversions: Vec<_> = field_idents
                .iter()
                .map(|field| quote! { #field: self.#field.into_static() })
                .collect();

            let extra_data_conversion = if has_extra_data {
                quote! { extra_data: self.extra_data.into_static(), }
            } else {
                quote! {}
            };

            quote! {
                impl jacquard_common::IntoStatic for #ident<'_> {
                    type Output = #ident<'static>;

                    fn into_static(self) -> Self::Output {
                        #ident {
                            #(#field_conversions,)*
                            #extra_data_conversion
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

    /// Generate IntoStatic impl for an enum
    pub(super) fn generate_into_static_for_enum(
        &self,
        type_name: &str,
        variant_info: &[(String, EnumVariantKind)],
        has_lifetime: bool,
        is_open: bool,
    ) -> TokenStream {
        let ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());

        if has_lifetime {
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
                impl jacquard_common::IntoStatic for #ident<'_> {
                    type Output = #ident<'static>;

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
