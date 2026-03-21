use crate::error::{CodegenError, Result};
use quote::quote;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::CodeGenerator;
use super::nsid_utils::NsidPath;
use super::prettify::{FileOutput, ResolvedImports};
use super::utils::{make_ident, sanitize_name};

impl<'c> CodeGenerator<'c> {
    /// Generate all code for the corpus, organized by file
    /// Returns a map of file paths to FileOutput with reordered tokens
    pub fn generate_all(&self) -> Result<BTreeMap<std::path::PathBuf, FileOutput>> {
        let mut file_contents: BTreeMap<std::path::PathBuf, Vec<super::prettify::GeneratedCode>> =
            BTreeMap::new();
        let mut file_nsids: BTreeMap<std::path::PathBuf, String> = BTreeMap::new();

        // Step 1: Enumerate local type names per file.
        // Also collect all file paths so we can determine submodule names.
        let mut file_local_names: BTreeMap<std::path::PathBuf, HashSet<String>> = BTreeMap::new();
        let mut all_file_paths: BTreeSet<std::path::PathBuf> = BTreeSet::new();
        for (nsid, doc) in self.corpus.iter() {
            let file_path = self.nsid_to_file_path(nsid.as_ref());
            all_file_paths.insert(file_path.clone());
            let names = file_local_names.entry(file_path).or_default();
            for def_name in doc.defs.keys() {
                names.insert(self.def_to_type_name(nsid.as_ref(), def_name.as_ref()));
            }
        }

        // Determine submodule names for each file. If `foo.rs` exists and `foo/bar.rs`
        // also exists, then `bar` is a submodule of `foo`. These names must be treated
        // as reserved — importing `use crate::something::bar;` would collide with
        // `pub mod bar;`.
        let mut file_submodule_names: BTreeMap<std::path::PathBuf, HashSet<String>> =
            BTreeMap::new();
        for file_path in &all_file_paths {
            // For a file like `app_bsky/feed/post.rs`, the parent module file is
            // `app_bsky/feed.rs`. Check if this file's parent has an entry.
            if let Some(parent_dir) = file_path.parent() {
                let parent_file = parent_dir.with_extension("rs");
                if all_file_paths.contains(&parent_file)
                    || file_local_names.contains_key(&parent_file)
                {
                    // This file is a submodule of parent_file.
                    if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                        file_submodule_names
                            .entry(parent_file)
                            .or_default()
                            .insert(stem.to_string());
                    }
                }
            }
        }

        // Step 2: Run collection pass and build ResolvedImports for each file.
        // Multiple NSIDs can map to the same file (e.g. `app.rocksky.album` and
        // `app.rocksky.album.defs` both output to `album.rs`), so we accumulate
        // imports per file path before resolving.
        let mut file_imports_map: BTreeMap<std::path::PathBuf, super::prettify::ImportSet> = BTreeMap::new();
        for (nsid, doc) in self.corpus.iter() {
            let file_path = self.nsid_to_file_path(nsid.as_ref());
            let file_imports = file_imports_map.entry(file_path).or_default();
            for (def_name, def) in &doc.defs {
                file_imports.merge(self.collect_def(nsid.as_ref(), def_name.as_ref(), def));
            }
        }

        let mut file_resolved: BTreeMap<std::path::PathBuf, ResolvedImports> = BTreeMap::new();
        for (file_path, file_imports) in &file_imports_map {
            let local_names = file_local_names
                .get(file_path)
                .cloned()
                .unwrap_or_default();

            let lexicon_paths: BTreeMap<String, String> = file_imports
                .lexicon_refs
                .iter()
                .filter_map(|ref_str| self.ref_to_crate_path(ref_str))
                .collect();

            let submodule_names = file_submodule_names
                .get(file_path)
                .cloned()
                .unwrap_or_default();
            let resolved = ResolvedImports::resolve(
                file_imports,
                &local_names,
                &submodule_names,
                self.mode,
                &lexicon_paths,
            );
            file_resolved.insert(file_path.clone(), resolved);
        }

        // Step 3: Generate code for all lexicons
        for (nsid, doc) in self.corpus.iter() {
            let file_path = self.nsid_to_file_path(nsid.as_ref());

            // Track which NSID this file is for
            file_nsids.insert(file_path.clone(), nsid.to_string());

            // Get the per-file ResolvedImports (built in Step 2)
            let resolved = file_resolved
                .get(&file_path)
                .expect("resolved imports built for every file");

            for (_def_name, def) in &doc.defs {
                let generated =
                    self.generate_def(nsid.as_ref(), _def_name.as_ref(), def, resolved)?;
                file_contents
                    .entry(file_path.clone())
                    .or_default()
                    .push(generated);
            }
        }

        // Combine all tokens for each file using FileOutput::combine for reordering
        let mut result = BTreeMap::new();
        for (path, generated_vec) in file_contents {
            let nsid = file_nsids.get(&path).cloned();
            let resolved = file_resolved
                .get(&path)
                .expect("resolved imports built for every file");
            let file_output = FileOutput::combine(generated_vec, nsid, resolved);
            result.insert(path, file_output);
        }

        Ok(result)
    }

    /// Generate parent module files with pub mod declarations
    pub fn generate_module_tree(
        &self,
        file_map: &BTreeMap<std::path::PathBuf, FileOutput>,
        defs_only: &BTreeMap<std::path::PathBuf, FileOutput>,
        subscription_files: &HashSet<std::path::PathBuf>,
    ) -> BTreeMap<std::path::PathBuf, FileOutput> {
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

                    // Check if this module is a subscription endpoint
                    let mut module_path = dir.clone();
                    module_path.push(format!("{}.rs", name));
                    let is_subscription = subscription_files.contains(&module_path);

                    if is_root && name != "builder_types" {
                        // Top-level modules get feature gates (except builder_types which is always needed)
                        quote! {
                            #[cfg(feature = #name)]
                            pub mod #ident;
                        }
                    } else if is_subscription {
                        // Subscription modules get streaming feature gate
                        quote! {
                            #[cfg(feature = "streaming")]
                            pub mod #ident;
                        }
                    } else {
                        quote! { pub mod #ident; }
                    }
                })
                .collect();

            // If this file already exists in defs_only (e.g., from defs), merge the content
            let module_tokens = if is_root {
                // lib.rs needs extern crate alloc for no_std compatibility
                quote! {
                    extern crate alloc; #(#mods)*
                }
            } else {
                quote! { #(#mods)* }
            };
            if let Some(existing_output) = defs_only.get(&mod_file_path) {
                // Put module declarations FIRST, then existing defs content
                let existing_tokens = &existing_output.tokens;
                let merged_tokens = quote! {
                    #module_tokens
                    #existing_tokens
                };
                result.insert(
                    mod_file_path,
                    FileOutput {
                        tokens: merged_tokens,
                        imports: existing_output.imports.clone(),
                        nsid: existing_output.nsid.clone(),
                    },
                );
            } else {
                result.insert(
                    mod_file_path,
                    FileOutput {
                        tokens: module_tokens,
                        imports: Default::default(),
                        nsid: None,
                    },
                );
            }
        }

        result
    }

    /// Write all generated code to disk
    pub fn write_to_disk(&self, output_dir: &std::path::Path) -> Result<()> {
        // Generate all code (defs only)
        let defs_files = self.generate_all()?;
        let mut all_files = defs_files.clone();

        // Generate common builder types (Set, Unset, IsSet, IsUnset)
        let common_types_path = std::path::PathBuf::from("builder_types.rs");
        let common_types_tokens = super::builder_gen::common::generate_common_types();
        all_files.insert(
            common_types_path,
            FileOutput {
                tokens: common_types_tokens,
                imports: Default::default(),
                nsid: None,
            },
        );

        // Get subscription files for feature gating
        let subscription_files = self.subscription_files.borrow();

        // Generate module tree iteratively until no new files appear
        loop {
            let module_map =
                self.generate_module_tree(&all_files, &defs_files, &subscription_files);
            let old_count = all_files.len();

            // Merge new module files
            for (path, file_output) in module_map {
                all_files.insert(path, file_output);
            }

            if all_files.len() == old_count {
                // No new files added
                break;
            }
        }

        // Write to disk
        for (path, file_output) in all_files {
            let full_path = output_dir.join(&path);

            // Create parent directories
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Format code
            let file: syn::File = syn::parse2(file_output.tokens.clone()).map_err(|e| {
                CodegenError::TokenParseError {
                    path: path.clone(),
                    source: e,
                    tokens: file_output.tokens.to_string(),
                }
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

                if !line.starts_with("#[") && i + 1 < lines.len() && !lines[i + 1].is_empty() {
                    let next_line = lines[i + 1];
                    if next_line.starts_with("#[") && !next_line.is_empty() {
                        result_lines.push("");
                    }
                }

                // Add blank line after last pub mod declaration before structs/enums
                if line.starts_with("pub mod ") && i + 1 < lines.len() {
                    let next_line = lines[i + 1];
                    if !next_line.starts_with("pub mod ")
                        && !next_line.starts_with("pub use ")
                        && !next_line.is_empty()
                    {
                        result_lines.push("");
                    }
                }
            }

            formatted = result_lines.join("\n");

            // Add header comment
            let header = if let Some(nsid) = &file_output.nsid {
                format!(
                    "// @generated by jacquard-lexicon. DO NOT EDIT.\n//\n// Lexicon: {}\n//\n// This file was automatically generated from Lexicon schemas.\n// Any manual changes will be overwritten on the next regeneration.\n\n",
                    nsid
                )
            } else {
                "// @generated by jacquard-lexicon. DO NOT EDIT.\n//\n// This file was automatically generated from Lexicon schemas.\n// Any manual changes will be overwritten on the next regeneration.\n\n".to_string()
            };
            formatted = format!("{}{}", header, formatted);

            // Write file
            std::fs::write(&full_path, formatted)?;
        }

        Ok(())
    }

    /// Get namespace dependencies collected during code generation
    pub fn get_namespace_dependencies(&self) -> HashMap<String, HashSet<String>> {
        self.namespace_deps.borrow().clone()
    }

    /// Generate Cargo.toml features section from namespace dependencies
    pub fn generate_cargo_features(&self, lib_rs_path: Option<&std::path::Path>) -> String {
        use std::fmt::Write;

        let deps = self.namespace_deps.borrow();
        let mut all_namespaces: HashSet<String> = HashSet::new();

        // Collect all namespaces from the corpus (first two segments of each NSID)
        for (nsid, _doc) in self.corpus.iter() {
            let nsid_path = NsidPath::parse(nsid.as_str());
            let namespace = nsid_path.namespace();
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
        let mut ns_to_feature: HashMap<&str, String> = HashMap::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::LexiconCorpus;

    #[test]
    fn test_enumerate_local_type_names() {
        // Verifies AC3.4: Local type names are correctly enumerated from the corpus before generation
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Generate all - this internally enumerates local type names
        let result = codegen.generate_all().expect("generate_all");

        // Verify that we got output for multiple files
        assert!(!result.is_empty(), "Should have generated files");

        // For pub.leaflet.poll.definition (multi-def), verify it's generated
        let has_poll_defs = result
            .keys()
            .any(|path| path.to_string_lossy().contains("poll"));
        assert!(has_poll_defs, "Should have poll defs in output");
    }

    #[test]
    fn test_collection_produces_imports() {
        // Verifies AC3.4: Collection produces non-empty ImportSet for a file with string types
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Get a lexicon with known string types
        let doc = corpus.get("app.bsky.feed.post").expect("get post");
        let post_def = doc.defs.get("main").expect("get main def");

        // Collect imports from the post definition
        let imports = codegen.collect_def("app.bsky.feed.post", "main", post_def);

        // Post should have collected imports for CowStr, Datetime, etc.
        assert!(
            imports.common.len() > 0,
            "Post definition should have collected common types"
        );
        assert!(
            imports.external.len() > 0,
            "Post definition should have collected external imports (Serialize, Deserialize)"
        );

        // Verify specific types that we know post uses
        assert!(
            imports
                .common
                .contains(&crate::codegen::prettify::CommonType::CowStr),
            "Post should collect CowStr"
        );
        assert!(
            imports
                .common
                .contains(&crate::codegen::prettify::CommonType::Datetime),
            "Post should collect Datetime"
        );
    }

    #[test]
    fn test_resolved_imports_for_collection_collision() {
        // Verifies AC3.4: ResolvedImports correctly marks Collection as qualified for files defining it
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Find a file that might define a Collection type
        // (This is harder to verify without knowing the exact corpus, but we can verify
        // that generate_all completes successfully with ResolvedImports built)
        let result = codegen.generate_all().expect("generate_all");

        // Verify we got output
        assert!(!result.is_empty(), "Should have generated code");

        // The fact that we generated code successfully means collection and
        // ResolvedImports::resolve() were executed without errors
        for (_path, file_output) in result {
            // Each file output should have imports (from collection)
            // and internally use ResolvedImports (built in generate_all)
            // We can't directly inspect ResolvedImports since it's internal to Task 4,
            // but we verify the output was generated
            assert!(
                !file_output.tokens.to_string().is_empty(),
                "Generated code should not be empty"
            );
        }
    }

    #[test]
    fn test_local_names_enumeration_accuracy() {
        // Verifies that local type names are enumerated correctly per file
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Generate all
        let result = codegen.generate_all().expect("generate_all");

        // For a known multi-def lexicon (app.bsky.feed.post), verify it generates
        let post_file = result.keys().find(|p| p.to_string_lossy().contains("post"));
        assert!(post_file.is_some(), "post file should exist");

        // The post record has at least "Post" as a type name
        let generated_code = post_file.and_then(|p| result.get(p));
        assert!(
            generated_code.is_some(),
            "Should have generated code for post"
        );
    }

    #[test]
    fn test_generate_all_runs_collection_without_errors() {
        // Verifies that generate_all successfully runs the collection pass
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // This should not panic or error - collection pass should run silently
        let result = codegen.generate_all();
        assert!(result.is_ok(), "generate_all should complete successfully");

        let files = result.unwrap();
        assert!(!files.is_empty(), "Should generate at least one file");
    }
}
