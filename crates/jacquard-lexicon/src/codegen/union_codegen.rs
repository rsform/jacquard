use crate::codegen::nsid_utils::{NsidPath, RefPath};
use crate::corpus::LexiconCorpus;
use crate::error::Result;
use heck::ToPascalCase;
use jacquard_common::CowStr;
use proc_macro2::TokenStream;
use quote::quote;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Information about a union variant
#[derive(Debug, Clone)]
pub struct UnionVariant {
    /// The original ref string (normalized)
    pub ref_str: String,
    /// The variant name (may be disambiguated)
    pub variant_name: String,
    /// The Rust type for this variant
    pub rust_type: TokenStream,
}

/// Context for tracking namespace dependencies during union generation
pub struct UnionGenContext<'a> {
    pub corpus: &'a LexiconCorpus,
    pub namespace_deps: &'a RefCell<HashMap<String, HashSet<String>>>,
    pub current_nsid: &'a str,
}

impl<'a> UnionGenContext<'a> {
    /// Build variants for a union with collision detection and disambiguation
    pub fn build_union_variants(
        &self,
        refs: &[CowStr<'static>],
        ref_to_rust_type: impl Fn(&str) -> Result<TokenStream>,
    ) -> Result<Vec<UnionVariant>> {
        let current_nsid_path = NsidPath::parse(self.current_nsid);
        let current_namespace = current_nsid_path.namespace();

        // First pass: collect all variant names and detect collisions
        #[derive(Debug)]
        struct VariantInfo {
            ref_str: String,
            ref_nsid: String,
            simple_name: String,
            is_current_namespace: bool,
        }

        let mut variant_infos = Vec::new();
        for ref_str in refs {
            let normalized_ref = RefPath::normalize(ref_str, self.current_nsid);
            let ref_path = RefPath::parse(&normalized_ref, None);
            let ref_nsid_str = ref_path.nsid();
            let ref_def = ref_path.def();

            // Skip unknown refs
            if !self.corpus.ref_exists(&normalized_ref) {
                continue;
            }

            let is_current_namespace = ref_nsid_str.starts_with(&current_namespace);
            let is_same_module = ref_nsid_str == self.current_nsid;

            // Generate simple variant name
            let last_segment = ref_nsid_str.split('.').last().unwrap();
            let simple_name = if ref_def == "main" {
                last_segment.to_pascal_case()
            } else if last_segment == "defs" {
                ref_def.to_pascal_case()
            } else if is_same_module {
                ref_def.to_pascal_case()
            } else {
                format!(
                    "{}{}",
                    last_segment.to_pascal_case(),
                    ref_def.to_pascal_case()
                )
            };

            variant_infos.push(VariantInfo {
                ref_str: normalized_ref.clone(),
                ref_nsid: ref_nsid_str.to_string(),
                simple_name,
                is_current_namespace,
            });
        }

        // Second pass: detect collisions and disambiguate
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        for info in &variant_infos {
            *name_counts.entry(info.simple_name.clone()).or_insert(0) += 1;
        }

        let mut variants = Vec::new();
        for info in variant_infos {
            let has_collision = name_counts.get(&info.simple_name).copied().unwrap_or(0) > 1;

            // Track namespace dependency for foreign refs
            if !info.is_current_namespace {
                let ref_nsid_path = NsidPath::parse(&info.ref_nsid);
                let foreign_namespace = ref_nsid_path.namespace();
                self.namespace_deps
                    .borrow_mut()
                    .entry(current_namespace.clone())
                    .or_default()
                    .insert(foreign_namespace);
            }

            // Disambiguate: add second NSID segment prefix only to foreign refs when there's a collision
            let variant_name = if has_collision && !info.is_current_namespace {
                let ref_nsid_path = NsidPath::parse(&info.ref_nsid);
                let segments = ref_nsid_path.segments();
                let prefix = if segments.len() >= 2 {
                    segments[1].to_pascal_case()
                } else {
                    segments[0].to_pascal_case()
                };
                format!("{}{}", prefix, info.simple_name)
            } else {
                info.simple_name.clone()
            };

            let rust_type = ref_to_rust_type(&info.ref_str)?;

            variants.push(UnionVariant {
                ref_str: info.ref_str,
                variant_name,
                rust_type,
            });
        }

        Ok(variants)
    }

    /// Build variants for a union without collision detection (simple mode)
    pub fn build_simple_union_variants(
        &self,
        refs: &[CowStr<'static>],
        ref_to_rust_type: impl Fn(&str) -> Result<TokenStream>,
    ) -> Result<Vec<UnionVariant>> {
        let mut variants = Vec::new();

        for ref_str in refs {
            let ref_str_s = ref_str.as_ref();
            let normalized_ref = RefPath::normalize(ref_str, self.current_nsid);
            let ref_path = RefPath::parse(&normalized_ref, None);
            let ref_nsid = ref_path.nsid();
            let ref_def = ref_path.def();

            let variant_name = if ref_def == "main" {
                let ref_nsid_path = NsidPath::parse(ref_nsid);
                ref_nsid_path.last_segment().to_pascal_case()
            } else {
                ref_def.to_pascal_case()
            };

            let rust_type = ref_to_rust_type(&normalized_ref)?;

            variants.push(UnionVariant {
                ref_str: ref_str_s.to_string(),
                variant_name,
                rust_type,
            });
        }

        Ok(variants)
    }
}

/// Generate variant tokens for a union enum
pub fn generate_variant_tokens(variants: &[UnionVariant]) -> Vec<TokenStream> {
    variants
        .iter()
        .map(|variant| {
            let variant_ident = syn::Ident::new(&variant.variant_name, proc_macro2::Span::call_site());
            let ref_str_literal = &variant.ref_str;
            let rust_type = &variant.rust_type;

            quote! {
                #[serde(rename = #ref_str_literal)]
                #variant_ident(Box<#rust_type>)
            }
        })
        .collect()
}
