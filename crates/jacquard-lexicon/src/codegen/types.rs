use crate::error::Result;
use crate::lexicon::{LexArrayItem, LexObjectProperty, LexString, LexStringFormat};
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;

use super::CodeGenerator;
use super::nsid_utils::{NsidPath, RefPath};
use super::prettify::{CommonType, ExternalImport, ResolvedImports};
use super::utils::{join_module_path, join_path_parts, namespace_prefix, sanitize_name_cow};

impl<'c> CodeGenerator<'c> {
    /// Convert a property type to Rust type
    pub(super) fn property_to_rust_type(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        prop: &LexObjectProperty<'static>,
        resolved: &ResolvedImports,
    ) -> Result<TokenStream> {
        match prop {
            LexObjectProperty::Boolean(_) => Ok(quote! { bool }),
            LexObjectProperty::Integer(_) => Ok(quote! { i64 }),
            LexObjectProperty::String(s) => {
                // If string has known_values, use the generated enum type
                if s.known_values.is_some() {
                    let enum_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    let enum_ident = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());
                    Ok(quote! { #enum_ident<S> })
                } else {
                    Ok(self.string_to_rust_type(s, resolved))
                }
            }
            LexObjectProperty::Bytes(_) => {
                Ok(resolved.external_type_tokens(&ExternalImport::Bytes))
            }
            LexObjectProperty::CidLink(_) => Ok(resolved.type_tokens(&CommonType::CidLink)),
            LexObjectProperty::Blob(_) => Ok(resolved.type_tokens(&CommonType::BlobRef)),
            LexObjectProperty::Unknown(_) => Ok(resolved.type_tokens(&CommonType::Data)),
            LexObjectProperty::Array(array) => {
                // For arrays with union items, check if multi-variant
                if let LexArrayItem::Union(union) = &array.items {
                    if union.refs.is_empty() {
                        // Empty union: fall back to Data.
                        let ty = resolved.type_tokens(&CommonType::Data);
                        Ok(quote! { Vec<#ty> })
                    } else if union.refs.len() == 1 {
                        // Single-variant: use the ref type directly
                        let ref_str = if union.refs[0].starts_with('#') {
                            format!("{}{}", nsid, union.refs[0])
                        } else {
                            union.refs[0].to_string()
                        };
                        let ref_type = self.ref_to_rust_type(&ref_str, resolved)?;
                        Ok(quote! { Vec<#ref_type> })
                    } else {
                        // Multi-variant: use generated union type
                        let union_name = self.generate_field_type_name(
                            nsid,
                            parent_type_name,
                            field_name,
                            "Item",
                        );
                        let union_ident =
                            syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                        Ok(quote! { Vec<#union_ident<S>> })
                    }
                } else {
                    let item_type = self.array_item_to_rust_type(nsid, &array.items, resolved)?;
                    Ok(quote! { Vec<#item_type> })
                }
            }
            LexObjectProperty::Object(object) => {
                // Empty objects (no properties) are untyped data bags.
                if object.properties.is_empty() {
                    return Ok(resolved.type_tokens(&CommonType::Data));
                }
                // Generate unique nested object type name with collision detection
                let object_name =
                    self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                let object_ident = syn::Ident::new(&object_name, proc_macro2::Span::call_site());
                Ok(quote! { #object_ident<S> })
            }
            LexObjectProperty::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                // Track namespace dependency for cross-namespace refs
                self.track_ref_namespace_dep(nsid, &ref_str);
                self.ref_to_rust_type(&ref_str, resolved)
            }
            LexObjectProperty::Union(union) => {
                if union.refs.is_empty() {
                    // Empty union: fall back to Data.
                    Ok(resolved.type_tokens(&CommonType::Data))
                } else if union.refs.len() == 1 {
                    // Check if this is a self-reference
                    let ref_str = if union.refs[0].starts_with('#') {
                        format!("{}{}", nsid, union.refs[0])
                    } else {
                        union.refs[0].to_string()
                    };

                    // Parse ref to get type name
                    let ref_path = RefPath::parse(&ref_str, None);
                    let ref_type_name = self.def_to_type_name(ref_path.nsid(), ref_path.def());

                    // If self-referential, keep union for indirection (variants are boxed)
                    if ref_type_name == parent_type_name {
                        let union_name =
                            self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                        let union_ident =
                            syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                        Ok(quote! { #union_ident<S> })
                    } else {
                        // Non-self-ref single-variant: use the ref type directly
                        // Track namespace dependency for cross-namespace refs
                        self.track_ref_namespace_dep(nsid, &ref_str);
                        self.ref_to_rust_type(&ref_str, resolved)
                    }
                } else {
                    // Multi-variant: generate union type with collision detection
                    let union_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    let union_ident = syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                    Ok(quote! { #union_ident<S> })
                }
            }
        }
    }

    /// Convert array item to Rust type
    pub(super) fn array_item_to_rust_type(
        &self,
        nsid: &str,
        item: &LexArrayItem,
        resolved: &ResolvedImports,
    ) -> Result<TokenStream> {
        match item {
            LexArrayItem::Boolean(_) => Ok(quote! { bool }),
            LexArrayItem::Integer(_) => Ok(quote! { i64 }),
            LexArrayItem::String(s) => Ok(self.string_to_rust_type(s, resolved)),
            LexArrayItem::Bytes(_) => Ok(resolved.external_type_tokens(&ExternalImport::Bytes)),
            LexArrayItem::CidLink(_) => Ok(resolved.type_tokens(&CommonType::CidLink)),
            LexArrayItem::Blob(_) => Ok(resolved.type_tokens(&CommonType::BlobRef)),
            LexArrayItem::Unknown(_) => Ok(resolved.type_tokens(&CommonType::Data)),
            LexArrayItem::Object(_) => {
                // For inline objects in arrays, use Data since we can't generate a unique type name.
                Ok(resolved.type_tokens(&CommonType::Data))
            }
            LexArrayItem::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                self.ref_to_rust_type(&ref_str, resolved)
            }
            LexArrayItem::Union(_) => {
                // For now, use Data.
                Ok(resolved.type_tokens(&CommonType::Data))
            }
        }
    }

    /// Convert string type to Rust type.
    /// Lifetimes are included by `type_tokens()` — callers must not add them.
    pub(super) fn string_to_rust_type(
        &self,
        s: &LexString,
        resolved: &ResolvedImports,
    ) -> TokenStream {
        match s.format {
            Some(LexStringFormat::Datetime) => resolved.type_tokens(&CommonType::Datetime),
            Some(LexStringFormat::Did) => resolved.type_tokens(&CommonType::Did),
            Some(LexStringFormat::Handle) => resolved.type_tokens(&CommonType::Handle),
            Some(LexStringFormat::AtIdentifier) => resolved.type_tokens(&CommonType::AtIdentifier),
            Some(LexStringFormat::Nsid) => resolved.type_tokens(&CommonType::Nsid),
            Some(LexStringFormat::AtUri) => resolved.type_tokens(&CommonType::AtUri),
            Some(LexStringFormat::Uri) => resolved.type_tokens(&CommonType::UriValue),
            Some(LexStringFormat::Cid) => resolved.type_tokens(&CommonType::Cid),
            Some(LexStringFormat::Language) => resolved.type_tokens(&CommonType::Language),
            Some(LexStringFormat::Tid) => resolved.type_tokens(&CommonType::Tid),
            Some(LexStringFormat::RecordKey) => resolved.type_tokens(&CommonType::RecordKey),
            _ => resolved.type_tokens(&CommonType::CowStr),
        }
    }

    /// Convert a ref string to its full crate path and type name.
    /// Returns `None` if the ref doesn't exist in the corpus.
    /// E.g. `"app.bsky.richtext.facet#main"` -> `("crate::app_bsky::richtext::facet::Facet", "Facet")`.
    /// Build the fully-qualified crate path and type name for a lexicon ref.
    /// Returns `None` if the ref doesn't exist in the corpus.
    ///
    /// The returned tuple is `(full_crate_path, type_name)` where:
    /// - `full_crate_path` is e.g. `"crate::app_bsky::richtext::facet::Facet"`
    /// - `type_name` is e.g. `"Facet"`
    fn build_ref_path(&self, ref_str: &str) -> Option<(String, String)> {
        let ref_path = RefPath::parse(ref_str, None);
        let ref_nsid = ref_path.nsid();
        let ref_def = ref_path.def();

        if !self.corpus.ref_exists(ref_str) {
            return None;
        }

        let nsid_path = NsidPath::parse(ref_nsid);
        let parts = nsid_path.segments();
        let last_segment = nsid_path.last_segment();
        let type_name = self.def_to_type_name(ref_nsid, ref_def);

        // defs types go in parent module (e.g. app.bsky.actor.defs#nux -> app_bsky::actor::Nux).
        let path_str = if nsid_path.is_defs() && parts.len() >= 3 {
            let first_two = namespace_prefix(parts[0], parts[1]);
            if parts.len() == 3 {
                join_path_parts(&[&self.root_module, &first_two, &type_name])
            } else {
                let middle = &parts[2..parts.len() - 1];
                let middle_path = join_module_path(middle);
                join_path_parts(&[&self.root_module, &first_two, &middle_path, &type_name])
            }
        } else {
            // Regular types go in their own module file.
            let (module_path, file_module) = if parts.len() >= 3 {
                let first_two = namespace_prefix(parts[0], parts[1]);
                let file_name = sanitize_name_cow(last_segment).to_string().to_snake_case();
                if parts.len() > 3 {
                    let middle = &parts[2..parts.len() - 1];
                    let middle_path = join_module_path(middle);
                    let base_path = join_path_parts(&[&first_two, &middle_path]);
                    (base_path, file_name)
                } else {
                    (first_two, file_name)
                }
            } else if parts.len() == 2 {
                let first = sanitize_name_cow(parts[0]).to_string();
                let file_name = sanitize_name_cow(parts[1]).to_string().to_snake_case();
                (first, file_name)
            } else {
                (parts[0].to_string(), "main".to_string())
            };
            join_path_parts(&[&self.root_module, &module_path, &file_module, &type_name])
        };

        Some((path_str, type_name))
    }

    /// Get the fully-qualified crate path and type name for a cross-namespace ref.
    /// Used by the collection pass to feed into `ResolvedImports::resolve()`.
    pub(super) fn ref_to_crate_path(&self, ref_str: &str) -> Option<(String, String)> {
        self.build_ref_path(ref_str)
    }

    /// Convert ref to Rust type path, using short names from resolved imports when available.
    pub(super) fn ref_to_rust_type(
        &self,
        ref_str: &str,
        resolved: &ResolvedImports,
    ) -> Result<TokenStream> {
        use crate::error::CodegenError;

        let Some((path_str, _type_name)) = self.build_ref_path(ref_str) else {
            return Ok(resolved.type_tokens(&CommonType::Data));
        };

        // In Pretty mode, check if this path was imported via a use statement.
        if let Some(short_tokens) = resolved.lexicon_ref_tokens(&path_str) {
            return if self.ref_needs_type_param(ref_str) {
                Ok(quote! { #short_tokens<S> })
            } else {
                Ok(quote! { #short_tokens })
            };
        }

        let path: syn::Path =
            syn::parse_str(&path_str).map_err(|e| CodegenError::PathParseError {
                path_str: path_str.clone(),
                source: e,
            })?;

        if self.ref_needs_type_param(ref_str) {
            Ok(quote! { #path<S> })
        } else {
            Ok(quote! { #path })
        }
    }
}
