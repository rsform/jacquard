use crate::error::Result;
use crate::lexicon::{LexArrayItem, LexObjectProperty, LexString, LexStringFormat};
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;

use super::CodeGenerator;
use super::nsid_utils::{NsidPath, RefPath};
use super::utils::{join_module_path, join_path_parts, namespace_prefix, sanitize_name_cow};

impl<'c> CodeGenerator<'c> {
    /// Convert a property type to Rust type
    pub(super) fn property_to_rust_type(
        &self,
        nsid: &str,
        parent_type_name: &str,
        field_name: &str,
        prop: &LexObjectProperty<'static>,
    ) -> Result<TokenStream> {
        match prop {
            LexObjectProperty::Boolean(_) => Ok(quote! { bool }),
            LexObjectProperty::Integer(_) => Ok(quote! { i64 }),
            LexObjectProperty::String(s) => Ok(self.string_to_rust_type(s)),
            LexObjectProperty::Bytes(_) => Ok(quote! { bytes::Bytes }),
            LexObjectProperty::CidLink(_) => {
                Ok(quote! { jacquard_common::types::cid::CidLink<'a> })
            }
            LexObjectProperty::Blob(_) => Ok(quote! { jacquard_common::types::blob::BlobRef<'a> }),
            LexObjectProperty::Unknown(_) => Ok(quote! { jacquard_common::types::value::Data<'a> }),
            LexObjectProperty::Array(array) => {
                // For arrays with union items, check if multi-variant
                if let LexArrayItem::Union(union) = &array.items {
                    if union.refs.is_empty() {
                        // Empty union: fall back to Data
                        Ok(quote! { Vec<jacquard_common::types::value::Data<'a>> })
                    } else if union.refs.len() == 1 {
                        // Single-variant: use the ref type directly
                        let ref_str = if union.refs[0].starts_with('#') {
                            format!("{}{}", nsid, union.refs[0])
                        } else {
                            union.refs[0].to_string()
                        };
                        let ref_type = self.ref_to_rust_type(&ref_str)?;
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
                        Ok(quote! { Vec<#union_ident<'a>> })
                    }
                } else {
                    let item_type = self.array_item_to_rust_type(nsid, &array.items)?;
                    Ok(quote! { Vec<#item_type> })
                }
            }
            LexObjectProperty::Object(object) => {
                // Empty objects (no properties) are untyped data bags
                if object.properties.is_empty() {
                    return Ok(quote! { jacquard_common::types::value::Data<'a> });
                }
                // Generate unique nested object type name with collision detection
                let object_name =
                    self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                let object_ident = syn::Ident::new(&object_name, proc_macro2::Span::call_site());
                Ok(quote! { #object_ident<'a> })
            }
            LexObjectProperty::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                self.ref_to_rust_type(&ref_str)
            }
            LexObjectProperty::Union(union) => {
                if union.refs.is_empty() {
                    // Empty union: fall back to Data
                    Ok(quote! { jacquard_common::types::value::Data<'a> })
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
                        Ok(quote! { #union_ident<'a> })
                    } else {
                        // Non-self-ref single-variant: use the ref type directly
                        self.ref_to_rust_type(&ref_str)
                    }
                } else {
                    // Multi-variant: generate union type with collision detection
                    let union_name =
                        self.generate_field_type_name(nsid, parent_type_name, field_name, "");
                    let union_ident = syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                    Ok(quote! { #union_ident<'a> })
                }
            }
        }
    }

    /// Convert array item to Rust type
    pub(super) fn array_item_to_rust_type(
        &self,
        nsid: &str,
        item: &LexArrayItem,
    ) -> Result<TokenStream> {
        match item {
            LexArrayItem::Boolean(_) => Ok(quote! { bool }),
            LexArrayItem::Integer(_) => Ok(quote! { i64 }),
            LexArrayItem::String(s) => Ok(self.string_to_rust_type(s)),
            LexArrayItem::Bytes(_) => Ok(quote! { bytes::Bytes }),
            LexArrayItem::CidLink(_) => Ok(quote! { jacquard_common::types::cid::CidLink<'a> }),
            LexArrayItem::Blob(_) => Ok(quote! { jacquard_common::types::blob::BlobRef<'a> }),
            LexArrayItem::Unknown(_) => Ok(quote! { jacquard_common::types::value::Data<'a> }),
            LexArrayItem::Object(_) => {
                // For inline objects in arrays, use Data since we can't generate a unique type name
                Ok(quote! { jacquard_common::types::value::Data<'a> })
            }
            LexArrayItem::Ref(ref_type) => {
                // Handle local refs (starting with #) by prepending the current NSID
                let ref_str = if ref_type.r#ref.starts_with('#') {
                    format!("{}{}", nsid, ref_type.r#ref)
                } else {
                    ref_type.r#ref.to_string()
                };
                self.ref_to_rust_type(&ref_str)
            }
            LexArrayItem::Union(_) => {
                // For now, use Data
                Ok(quote! { jacquard_common::types::value::Data<'a> })
            }
        }
    }

    /// Convert string type to Rust type
    pub(super) fn string_to_rust_type(&self, s: &LexString) -> TokenStream {
        match s.format {
            Some(LexStringFormat::Datetime) => {
                quote! { jacquard_common::types::string::Datetime }
            }
            Some(LexStringFormat::Did) => quote! { jacquard_common::types::string::Did<'a> },
            Some(LexStringFormat::Handle) => quote! { jacquard_common::types::string::Handle<'a> },
            Some(LexStringFormat::AtIdentifier) => {
                quote! { jacquard_common::types::ident::AtIdentifier<'a> }
            }
            Some(LexStringFormat::Nsid) => quote! { jacquard_common::types::string::Nsid<'a> },
            Some(LexStringFormat::AtUri) => quote! { jacquard_common::types::string::AtUri<'a> },
            Some(LexStringFormat::Uri) => quote! { jacquard_common::types::string::Uri<'a> },
            Some(LexStringFormat::Cid) => quote! { jacquard_common::types::string::Cid<'a> },
            Some(LexStringFormat::Language) => {
                quote! { jacquard_common::types::string::Language }
            }
            Some(LexStringFormat::Tid) => quote! { jacquard_common::types::string::Tid },
            Some(LexStringFormat::RecordKey) => {
                quote! { jacquard_common::types::string::RecordKey<jacquard_common::types::string::Rkey<'a>> }
            }
            _ => quote! { jacquard_common::CowStr<'a> },
        }
    }

    /// Convert ref to Rust type path
    pub(super) fn ref_to_rust_type(&self, ref_str: &str) -> Result<TokenStream> {
        use crate::error::CodegenError;

        // Parse ref to get NSID and def
        let ref_path = RefPath::parse(ref_str, None);
        let ref_nsid = ref_path.nsid();
        let ref_def = ref_path.def();

        // Check if ref exists
        if !self.corpus.ref_exists(ref_str) {
            // Fallback to Data
            return Ok(quote! { jacquard_common::types::value::Data<'a> });
        }

        // Parse NSID into components
        let nsid_path = NsidPath::parse(ref_nsid);
        let parts = nsid_path.segments();
        let last_segment = nsid_path.last_segment();

        // Convert NSID to module path
        // com.atproto.repo.strongRef -> com_atproto::repo::strong_ref::StrongRef
        // app.bsky.richtext.facet -> app_bsky::richtext::facet::Facet
        // app.bsky.actor.defs#nux -> app_bsky::actor::Nux (defs go in parent module)
        let type_name = self.def_to_type_name(ref_nsid, ref_def);

        let path_str = if nsid_path.is_defs() && parts.len() >= 3 {
            // defs types go in parent module
            let first_two = namespace_prefix(parts[0], parts[1]);
            if parts.len() == 3 {
                // com.atproto.defs -> com_atproto::TypeName
                join_path_parts(&[&self.root_module, &first_two, &type_name])
            } else {
                // app.bsky.actor.defs -> app_bsky::actor::TypeName
                let middle = &parts[2..parts.len() - 1];
                let middle_path = join_module_path(middle);
                join_path_parts(&[&self.root_module, &first_two, &middle_path, &type_name])
            }
        } else {
            // Regular types go in their own module file
            let (module_path, file_module) = if parts.len() >= 3 {
                // Join first two segments with underscore
                let first_two = namespace_prefix(parts[0], parts[1]);
                let file_name = sanitize_name_cow(last_segment).to_string().to_snake_case();

                if parts.len() > 3 {
                    // Middle segments form the module path
                    let middle = &parts[2..parts.len() - 1];
                    let middle_path = join_module_path(middle);
                    let base_path = join_path_parts(&[&first_two, &middle_path]);
                    (base_path, file_name)
                } else {
                    // Only 3 parts: com.atproto.label -> com_atproto, file: label
                    (first_two, file_name)
                }
            } else if parts.len() == 2 {
                // e.g., "com.example" -> "com_example", file: example
                let first = sanitize_name_cow(parts[0]).to_string();
                let file_name = sanitize_name_cow(parts[1]).to_string().to_snake_case();
                (first, file_name)
            } else {
                (parts[0].to_string(), "main".to_string())
            };

            join_path_parts(&[&self.root_module, &module_path, &file_module, &type_name])
        };

        let path: syn::Path = syn::parse_str(&path_str).map_err(|e| CodegenError::PathParseError {
            path_str: path_str.clone(),
            source: e,
        })?;

        // Only add lifetime if the target type needs it
        if self.ref_needs_lifetime(ref_str) {
            Ok(quote! { #path<'a> })
        } else {
            Ok(quote! { #path })
        }
    }
}
