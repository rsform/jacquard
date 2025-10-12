use crate::error::{CodegenError, Result};
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::utils::{make_ident, sanitize_name};
use super::CodeGenerator;

impl<'c> CodeGenerator<'c> {
    /// Generate all code for the corpus, organized by file
    /// Returns a map of file paths to (tokens, optional NSID)
    pub fn generate_all(
        &self,
    ) -> Result<BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)>> {
        let mut file_contents: BTreeMap<std::path::PathBuf, Vec<TokenStream>> = BTreeMap::new();
        let mut file_nsids: BTreeMap<std::path::PathBuf, String> = BTreeMap::new();

        // Generate code for all lexicons
        for (nsid, doc) in self.corpus.iter() {
            let file_path = self.nsid_to_file_path(nsid.as_ref());

            // Track which NSID this file is for
            file_nsids.insert(file_path.clone(), nsid.to_string());

            for (def_name, def) in &doc.defs {
                let tokens = self.generate_def(nsid.as_ref(), def_name.as_ref(), def)?;
                file_contents
                    .entry(file_path.clone())
                    .or_default()
                    .push(tokens);
            }
        }

        // Combine all tokens for each file
        let mut result = BTreeMap::new();
        for (path, tokens_vec) in file_contents {
            let nsid = file_nsids.get(&path).cloned();
            result.insert(path, (quote! { #(#tokens_vec)* }, nsid));
        }

        Ok(result)
    }

    /// Generate parent module files with pub mod declarations
    pub fn generate_module_tree(
        &self,
        file_map: &BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)>,
        defs_only: &BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)>,
    ) -> BTreeMap<std::path::PathBuf, (TokenStream, Option<String>)> {
        // Track what modules each directory needs to declare
        // Key: directory path, Value: set of module names (file stems)
        let mut dir_modules: BTreeMap<std::path::PathBuf, BTreeSet<String>> = BTreeMap::new();

        // Collect all parent directories that have files
        let mut all_dirs: BTreeSet<std::path::PathBuf> = BTreeSet::new();
        for path in file_map.keys() {
            if let Some(parent_dir) = path.parent() {
                all_dirs.insert(parent_dir.to_path_buf());
            }
        }

        for path in file_map.keys() {
            if let Some(parent_dir) = path.parent() {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Skip mod.rs and lib.rs - they're module files, not modules to declare
                    if file_stem == "mod" || file_stem == "lib" {
                        continue;
                    }

                    // Always add the module declaration to parent
                    dir_modules
                        .entry(parent_dir.to_path_buf())
                        .or_default()
                        .insert(file_stem.to_string());
                }
            }
        }

        // Generate module files
        let mut result = BTreeMap::new();

        for (dir, module_names) in dir_modules {
            let mod_file_path = if dir.components().count() == 0 {
                // Root directory -> lib.rs for library crates
                std::path::PathBuf::from("lib.rs")
            } else {
                // Subdirectory: app_bsky/feed -> app_bsky/feed.rs (Rust 2018 style)
                let dir_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("mod");
                let sanitized_dir_name = sanitize_name(dir_name);
                let mut path = dir
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""))
                    .to_path_buf();
                path.push(format!("{}.rs", sanitized_dir_name));
                path
            };

            let is_root = dir.components().count() == 0;
            let mods: Vec<_> = module_names
                .iter()
                .map(|name| {
                    let ident = make_ident(name);
                    if is_root {
                        // Top-level modules get feature gates
                        quote! {
                            #[cfg(feature = #name)]
                            pub mod #ident;
                        }
                    } else {
                        quote! { pub mod #ident; }
                    }
                })
                .collect();

            // If this file already exists in defs_only (e.g., from defs), merge the content
            let module_tokens = quote! { #(#mods)* };
            if let Some((existing_tokens, nsid)) = defs_only.get(&mod_file_path) {
                // Put module declarations FIRST, then existing defs content
                result.insert(
                    mod_file_path,
                    (quote! { #module_tokens #existing_tokens }, nsid.clone()),
                );
            } else {
                result.insert(mod_file_path, (module_tokens, None));
            }
        }

        result
    }

    /// Write all generated code to disk
    pub fn write_to_disk(&self, output_dir: &std::path::Path) -> Result<()> {
        // Generate all code (defs only)
        let defs_files = self.generate_all()?;
        let mut all_files = defs_files.clone();

        // Generate module tree iteratively until no new files appear
        loop {
            let module_map = self.generate_module_tree(&all_files, &defs_files);
            let old_count = all_files.len();

            // Merge new module files
            for (path, tokens) in module_map {
                all_files.insert(path, tokens);
            }

            if all_files.len() == old_count {
                // No new files added
                break;
            }
        }

        // Write to disk
        for (path, (tokens, nsid)) in all_files {
            let full_path = output_dir.join(&path);

            // Create parent directories
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CodegenError::Other {
                    message: format!("Failed to create directory {:?}: {}", parent, e),
                    source: None,
                })?;
            }

            // Format code
            let file: syn::File = syn::parse2(tokens.clone()).map_err(|e| CodegenError::Other {
                message: format!(
                    "Failed to parse tokens for {:?}: {}\nTokens: {}",
                    path, e, tokens
                ),
                source: None,
            })?;
            let mut formatted = prettyplease::unparse(&file);

            // Add blank lines between top-level items for better readability
            let lines: Vec<&str> = formatted.lines().collect();
            let mut result_lines = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                result_lines.push(*line);

                // Add blank line after closing braces that are at column 0 (top-level items)
                if *line == "}" && i + 1 < lines.len() && !lines[i + 1].is_empty() {
                    result_lines.push("");
                }

                // Add blank line after last pub mod declaration before structs/enums
                if line.starts_with("pub mod ") && i + 1 < lines.len() {
                    let next_line = lines[i + 1];
                    if !next_line.starts_with("pub mod ") && !next_line.is_empty() {
                        result_lines.push("");
                    }
                }
            }

            formatted = result_lines.join("\n");

            // Add header comment
            let header = if let Some(nsid) = nsid {
                format!(
                    "// @generated by jacquard-lexicon. DO NOT EDIT.\n//\n// Lexicon: {}\n//\n// This file was automatically generated from Lexicon schemas.\n// Any manual changes will be overwritten on the next regeneration.\n\n",
                    nsid
                )
            } else {
                "// @generated by jacquard-lexicon. DO NOT EDIT.\n//\n// This file was automatically generated from Lexicon schemas.\n// Any manual changes will be overwritten on the next regeneration.\n\n".to_string()
            };
            formatted = format!("{}{}", header, formatted);

            // Write file
            std::fs::write(&full_path, formatted).map_err(|e| CodegenError::Other {
                message: format!("Failed to write file {:?}: {}", full_path, e),
                source: None,
            })?;
        }

        Ok(())
    }

    /// Get namespace dependencies collected during code generation
    pub fn get_namespace_dependencies(
        &self,
    ) -> HashMap<String, HashSet<String>> {
        self.namespace_deps.borrow().clone()
    }

    /// Generate Cargo.toml features section from namespace dependencies
    pub fn generate_cargo_features(&self, lib_rs_path: Option<&std::path::Path>) -> String {
        use std::fmt::Write;

        let deps = self.namespace_deps.borrow();
        let mut all_namespaces: HashSet<String> =
            HashSet::new();

        // Collect all namespaces from the corpus (first two segments of each NSID)
        for (nsid, _doc) in self.corpus.iter() {
            let parts: Vec<_> = nsid.as_str().splitn(3, '.').collect();
            let namespace = if parts.len() >= 2 {
                format!("{}.{}", parts[0], parts[1])
            } else {
                nsid.to_string()
            };
            all_namespaces.insert(namespace);
        }

        // Also collect existing feature names from lib.rs
        let mut existing_features = HashSet::new();
        if let Some(lib_rs) = lib_rs_path {
            if let Ok(content) = std::fs::read_to_string(lib_rs) {
                for line in content.lines() {
                    if let Some(feature) = line
                        .trim()
                        .strip_prefix("#[cfg(feature = \"")
                        .and_then(|s| s.strip_suffix("\")]"))
                    {
                        existing_features.insert(feature.to_string());
                    }
                }
            }
        }

        let mut output = String::new();
        writeln!(&mut output, "# Generated namespace features").unwrap();

        // Convert namespace to feature name (matching module path sanitization)
        let to_feature_name = |ns: &str| {
            ns.split('.')
                .map(|segment| {
                    // Apply same sanitization as module names
                    let mut result = segment.replace('-', "_");
                    // Prefix with underscore if starts with digit
                    if result.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        result.insert(0, '_');
                    }
                    result
                })
                .collect::<Vec<_>>()
                .join("_")
        };

        // Collect all feature names (from corpus + existing lib.rs)
        let mut all_feature_names = HashSet::new();
        for ns in &all_namespaces {
            all_feature_names.insert(to_feature_name(ns));
        }
        all_feature_names.extend(existing_features);

        // Sort for consistent output
        let mut feature_names: Vec<_> = all_feature_names.iter().collect();
        feature_names.sort();

        // Map namespace to feature name for dependency lookup
        let mut ns_to_feature: HashMap<&str, String> =
            HashMap::new();
        for ns in &all_namespaces {
            ns_to_feature.insert(ns.as_str(), to_feature_name(ns));
        }

        for feature_name in feature_names {
            // Find corresponding namespace for this feature (if any) to look up deps
            let feature_deps: Vec<String> = all_namespaces
                .iter()
                .find(|ns| to_feature_name(ns) == *feature_name)
                .and_then(|ns| deps.get(ns.as_str()))
                .map(|ns_deps| {
                    let mut dep_features: Vec<_> = ns_deps
                        .iter()
                        .map(|d| format!("\"{}\"", to_feature_name(d)))
                        .collect();
                    dep_features.sort();
                    dep_features
                })
                .unwrap_or_default();

            if !feature_deps.is_empty() {
                writeln!(
                    &mut output,
                    "{} = [{}]",
                    feature_name,
                    feature_deps.join(", ")
                )
                .unwrap();
            } else {
                writeln!(&mut output, "{} = []", feature_name).unwrap();
            }
        }

        output
    }
}
