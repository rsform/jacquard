use crate::error::Result;
use crate::lexicon::{
    LexArrayItem, LexInteger, LexObject, LexObjectProperty, LexRecord, LexString,
};
use heck::ToSnakeCase;
use jacquard_common::smol_str::SmolStr;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeMap;

use super::CodeGenerator;
use super::utils::{make_ident, value_to_variant_name};

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
    ) -> Result<Vec<TokenStream>> {
        let mut nested = Vec::new();

        for (field_name, field_type) in properties {
            match field_type {
                LexObjectProperty::Union(union) => {
                    // Skip empty, single-variant unions unless they're self-referential
                    if !union.refs.is_empty()
                        && (union.refs.len() > 1
                            || self.is_self_referential_union(nsid, parent_type_name, &union))
                    {
                        let union_name =
                            self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                        let refs: Vec<_> = union.refs.iter().cloned().collect();
                        let union_def =
                            self.generate_union(nsid, &union_name, &refs, None, union.closed)?;
                        nested.push(union_def);
                    }
                }
                LexObjectProperty::Object(nested_obj) if include_nested_objects => {
                    let object_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    let obj_def = self.generate_object(nsid, &object_name, &nested_obj)?;
                    nested.push(obj_def);
                }
                LexObjectProperty::Array(array) => {
                    if let LexArrayItem::Union(union) = &array.items {
                        // Skip single-variant array unions
                        if union.refs.len() > 1 {
                            let union_name = self.generate_field_type_name(
                                nsid,
                                parent_type_name,
                                field_name,
                                "Item",
                            );
                            let refs: Vec<_> = union.refs.iter().cloned().collect();
                            let union_def =
                                self.generate_union(nsid, &union_name, &refs, None, union.closed)?;
                            nested.push(union_def);
                        }
                    }
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
    ) -> Result<TokenStream> {
        match &record.record {
            crate::lexicon::LexRecordRecord::Object(obj) => {
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

                // Records always get a lifetime since they have the #[lexicon] attribute
                // which adds extra_data: BTreeMap<..., Data<'a>>
                // Skip custom builder for types that conflict with the macro's unqualified type references
                let has_builder =
                    !super::builder_heuristics::conflicts_with_builder_macro(&type_name);

                // Generate main struct fields
                let fields = self.generate_object_fields(nsid, &type_name, obj, has_builder)?;
                let doc = self.generate_doc_comment(record.description.as_ref());

                let struct_def = quote! {
                    #doc
                    #[jacquard_derive::lexicon]
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
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
                let unions = self.generate_nested_types(nsid, &type_name, &obj.properties, true)?;

                // Generate typed GetRecordOutput wrapper
                let output_type_name = format!("{}GetRecordOutput", type_name);
                let output_type_ident =
                    syn::Ident::new(&output_type_name, proc_macro2::Span::call_site());

                let output_wrapper = quote! {
                    /// Typed wrapper for GetRecord response with this collection's record type.
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                    #[serde(rename_all = "camelCase")]
                    pub struct #output_type_ident<'a> {
                        #[serde(skip_serializing_if = "std::option::Option::is_none")]
                        #[serde(borrow)]
                        pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
                        #[serde(borrow)]
                        pub uri: jacquard_common::types::string::AtUri<'a>,
                        #[serde(borrow)]
                        pub value: #ident<'a>,
                    }
                };

                // Generate marker struct for XrpcResp
                let record_marker_name = format!("{}Record", type_name);
                let record_marker_ident =
                    syn::Ident::new(&record_marker_name, proc_macro2::Span::call_site());

                let record_marker = quote! {
                    /// Marker type for deserializing records from this collection.
                    #[derive(Debug, serde::Serialize, serde::Deserialize)]
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

                // Generate Collection trait impl
                let collection_impl = quote! {
                    impl jacquard_common::types::collection::Collection for #ident<'_> {
                        const NSID: &'static str = #nsid;
                        type Record = #record_marker_ident;
                    }
                };

                // Generate collection impl for the marker struct to drive fetch_record()
                let collection_marker_impl = quote! {
                    impl jacquard_common::types::collection::Collection for #record_marker_ident {
                        const NSID: &'static str = #nsid;
                        type Record = #record_marker_ident;
                    }
                };

                // Generate LexiconSchema impl with shared lexicon_doc function
                let (shared_fn, schema_impl) =
                    self.generate_schema_impl_with_shared(&type_name, nsid, "main", true);

                Ok(quote! {
                    #struct_def
                    #builder

                    impl<'a> #ident<'a> {
                        pub fn uri(uri: impl Into<jacquard_common::CowStr<'a>>) -> Result<jacquard_common::types::uri::RecordUri<'a, #record_marker_ident>, jacquard_common::types::uri::UriError> {
                            jacquard_common::types::uri::RecordUri::try_from_uri(jacquard_common::types::string::AtUri::new_cow(uri.into())?)
                        }
                    }
                    #(#unions)*
                    #output_wrapper
                    #from_impl
                    #collection_impl
                    #record_marker
                    #collection_marker_impl
                    #schema_impl
                    #shared_fn
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
    ) -> Result<TokenStream> {
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
        let has_default = decision.has_default;

        let fields = self.generate_object_fields(nsid, &type_name, obj, has_builder)?;
        let doc = self.generate_doc_comment(obj.description.as_ref());

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

        // Generate custom builder if needed
        let builder = if has_builder {
            let ctx = super::builder_gen::BuilderGenContext::from_object(
                self, nsid, &type_name, obj, true, // objects always have lifetime
            );
            ctx.generate()
        } else {
            quote! {}
        };

        // Generate union types and nested object types for this object
        let unions = self.generate_nested_types(nsid, &type_name, &obj.properties, true)?;

        // Generate LexiconSchema impl with shared lexicon_doc function
        let (shared_fn, schema_impl) =
            self.generate_schema_impl_with_shared(&type_name, nsid, def_name, true);

        Ok(quote! {
            #struct_def
            #builder
            #(#unions)*
            #shared_fn
            #schema_impl
        })
    }

    /// Generate fields for an object
    pub(super) fn generate_object_fields(
        &self,
        nsid: &str,
        parent_type_name: &str,
        obj: &LexObject<'static>,
        _is_builder: bool,
    ) -> Result<TokenStream> {
        let required = obj.required.as_ref().map(|r| r.as_slice()).unwrap_or(&[]);
        let nullable = obj.nullable.as_ref().map(|n| n.as_slice()).unwrap_or(&[]);

        let mut fields = Vec::new();
        for (field_name, field_type) in &obj.properties {
            let is_required = required.contains(field_name);
            let is_nullable = nullable.contains(field_name);
            let field_tokens = self.generate_field(
                nsid,
                parent_type_name,
                field_name,
                field_type,
                is_required,
                is_nullable,
            )?;
            fields.push(field_tokens);
        }

        Ok(quote! { #(#fields)* })
    }

    /// Generate a single field
    pub(super) fn generate_field(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        field_type: &LexObjectProperty<'static>,
        is_required: bool,
        is_nullable: bool,
    ) -> Result<TokenStream> {
        if field_name.is_empty() {
            eprintln!(
                "Warning: Empty field name in lexicon '{}' type '{}', using 'unknown' as fallback",
                nsid, parent_type_name
            );
        }
        let field_ident = make_ident(&field_name.to_snake_case());

        let rust_type =
            self.property_to_rust_type(nsid, parent_type_name, field_name, field_type)?;
        let needs_lifetime = self.property_needs_lifetime(field_type);

        let rust_type = if is_required && !is_nullable {
            rust_type
        } else {
            quote! { std::option::Option<#rust_type> }
        };

        // Extract description from field type
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
        let doc = self.generate_doc_comment(description);

        let mut attrs = Vec::new();

        if !is_required {
            attrs.push(quote! { #[serde(skip_serializing_if = "std::option::Option::is_none")] });
        }

        // Add serde(borrow) to all fields with lifetimes
        if needs_lifetime {
            attrs.push(quote! { #[serde(borrow)] });
        }

        if matches!(field_type, LexObjectProperty::Bytes(_)) {
            if is_required {
                attrs.push(quote! { #[serde(with = "jacquard_common::serde_bytes_helper")] });
            } else {
                attrs.push(
                    quote! {#[serde(default, with = "jacquard_common::opt_serde_bytes_helper")] },
                );
            }
        }

        Ok(quote! {
            #doc
            #(#attrs)*
            pub #field_ident: #rust_type,
        })
    }

    /// Generate a union enum for refs
    pub fn generate_union(
        &self,
        current_nsid: &str,
        union_name: &str,
        refs: &[jacquard_common::CowStr<'static>],
        description: Option<&str>,
        closed: Option<bool>,
    ) -> Result<TokenStream> {
        let enum_ident = syn::Ident::new(union_name, proc_macro2::Span::call_site());

        // Build variants using the union_codegen module
        let ctx = super::union_codegen::UnionGenContext {
            corpus: self.corpus,
            namespace_deps: &self.namespace_deps,
            current_nsid,
        };

        let union_variants =
            ctx.build_union_variants(refs, |ref_str| self.ref_to_rust_type(ref_str))?;
        let variants = super::union_codegen::generate_variant_tokens(&union_variants);

        let doc = description
            .map(|d| quote! { #[doc = #d] })
            .unwrap_or_else(|| quote! {});

        // Only add open_union if not closed
        let is_open = closed != Some(true);

        if is_open {
            Ok(quote! {
                #doc
                #[jacquard_derive::open_union]
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(tag = "$type")]
                #[serde(bound(deserialize = "'de: 'a"))]
                pub enum #enum_ident<'a> {
                    #(#variants,)*
                }
            })
        } else {
            Ok(quote! {
                #doc
                #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, jacquard_derive::IntoStatic)]
                #[serde(tag = "$type")]
                #[serde(bound(deserialize = "'de: 'a"))]
                pub enum #enum_ident<'a> {
                    #(#variants,)*
                }
            })
        }
    }

    /// Generate enum for string with known values
    pub(super) fn generate_known_values_enum(
        &self,
        nsid: &str,
        def_name: &str,
        string: &LexString<'static>,
    ) -> Result<TokenStream> {
        let type_name = self.def_to_type_name(nsid, def_name);
        let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());

        let known_values = string.known_values.as_ref().unwrap();
        let mut variants = Vec::new();
        let mut from_str_arms = Vec::new();
        let mut as_str_arms = Vec::new();

        for value in known_values {
            // Convert value to valid Rust identifier
            let value_str = value.as_ref();
            let variant_name = value_to_variant_name(value_str);
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

        let doc = self.generate_doc_comment(string.description.as_ref());

        // Generate IntoStatic impl
        let variant_info: Vec<(String, EnumVariantKind)> = known_values
            .iter()
            .map(|value| {
                let variant_name = value_to_variant_name(value.as_ref());
                (variant_name, EnumVariantKind::Unit)
            })
            .chain(std::iter::once((
                "Other".to_string(),
                EnumVariantKind::Tuple,
            )))
            .collect();
        let into_static_impl =
            self.generate_into_static_for_enum(&type_name, &variant_info, true, false);

        Ok(quote! {
            #doc
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub enum #ident<'a> {
                #(#variants,)*
                Other(jacquard_common::CowStr<'a>),
            }

            impl<'a> #ident<'a> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms,)*
                        Self::Other(s) => s.as_ref(),
                    }
                }
            }

            impl<'a> From<&'a str> for #ident<'a> {
                fn from(s: &'a str) -> Self {
                    match s {
                        #(#from_str_arms,)*
                        _ => Self::Other(jacquard_common::CowStr::from(s)),
                    }
                }
            }

            impl<'a> From<String> for #ident<'a> {
                fn from(s: String) -> Self {
                    match s.as_str() {
                        #(#from_str_arms,)*
                        _ => Self::Other(jacquard_common::CowStr::from(s)),
                    }
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

            #into_static_impl
        })
    }

    /// Generate enum for integer with enum values
    pub(super) fn generate_integer_enum(
        &self,
        nsid: &str,
        def_name: &str,
        integer: &LexInteger<'static>,
    ) -> Result<TokenStream> {
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

        Ok(quote! {
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
        })
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
