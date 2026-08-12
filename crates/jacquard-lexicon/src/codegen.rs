use crate::corpus::LexiconCorpus;
use crate::error::{CodegenError, Result};
use crate::lexicon::{LexArrayItem, LexUserType};
use proc_macro2::TokenStream;
use quote::quote;

pub(crate) mod builder_gen;
pub(crate) mod builder_heuristics;
pub(crate) mod collect;
pub(crate) mod names;
pub(crate) mod output;
pub(crate) mod prettify;
pub(crate) mod schema_impl;
pub(crate) mod structs;
pub(crate) mod type_param;
pub(crate) mod types;
pub(crate) mod union_codegen;
pub(crate) mod utils;
pub(crate) mod xrpc;

// Re-export types that external consumers need (binaries, test crates).
pub use prettify::{CodegenMode, FileOutput, GeneratedCode};

/// Code generator for lexicon types
pub struct CodeGenerator<'c> {
    corpus: &'c LexiconCorpus,
    root_module: String,
    mode: prettify::CodegenMode,
    /// Track namespace dependencies (namespace -> set of namespaces it depends on)
    namespace_deps:
        std::cell::RefCell<std::collections::HashMap<String, std::collections::HashSet<String>>>,
    /// Track which file paths contain subscription endpoints
    subscription_files: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>>,
    /// Track which NSIDs have already generated their shared lexicon_doc function
    generated_shared_docs: std::cell::RefCell<std::collections::HashSet<String>>,
}

impl<'c> CodeGenerator<'c> {
    /// Create a new code generator
    pub fn new(corpus: &'c LexiconCorpus, root_module: impl Into<String>) -> Self {
        Self {
            corpus,
            root_module: root_module.into(),
            mode: prettify::CodegenMode::Macro,
            namespace_deps: std::cell::RefCell::new(std::collections::HashMap::new()),
            subscription_files: std::cell::RefCell::new(std::collections::HashSet::new()),
            generated_shared_docs: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Create a new code generator with a specific codegen mode
    pub fn with_mode(
        corpus: &'c LexiconCorpus,
        root_module: impl Into<String>,
        mode: prettify::CodegenMode,
    ) -> Self {
        Self {
            corpus,
            root_module: root_module.into(),
            mode,
            namespace_deps: std::cell::RefCell::new(std::collections::HashMap::new()),
            subscription_files: std::cell::RefCell::new(std::collections::HashSet::new()),
            generated_shared_docs: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Generate doc comment from optional description (wrapper for utils function)
    fn generate_doc_comment(&self, desc: Option<&jacquard_common::CowStr>) -> TokenStream {
        utils::generate_doc_comment(desc)
    }

    /// Create a ResolvedImports instance for this generator's mode with no collisions.
    /// Used in tests and as a fallback when per-file ResolvedImports isn't available.
    /// In Macro mode, this produces fully-qualified paths for all types.
    /// In Pretty mode, this produces short names (but should normally use per-file ResolvedImports from Phase 2).
    #[cfg(test)]
    pub(crate) fn default_resolved_imports(&self) -> prettify::ResolvedImports {
        prettify::ResolvedImports::resolve(
            &prettify::ImportSet::default(),
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            self.mode,
            &std::collections::BTreeMap::new(),
        )
    }

    /// Track namespace dependency when a ref crosses namespace boundaries
    pub(crate) fn track_ref_namespace_dep(&self, current_nsid: &str, ref_str: &str) {
        use crate::ref_utils::{NsidPath, RefPath};

        let current_path = NsidPath::parse(current_nsid);
        let ref_path = RefPath::parse(ref_str, None);
        let ref_nsid_path = NsidPath::parse(ref_path.nsid());

        let current_ns = current_path.namespace();
        let ref_ns = ref_nsid_path.namespace();

        // Only track if crossing namespace boundaries
        if current_ns != ref_ns {
            self.namespace_deps
                .borrow_mut()
                .entry(current_ns)
                .or_default()
                .insert(ref_ns);
        }
    }

    /// Generate or reference the shared lexicon_doc function for this NSID.
    /// Returns (optional shared function, trait impl tokens).
    pub(crate) fn generate_schema_impl_with_shared(
        &self,
        type_name: &str,
        nsid: &str,
        def_name: &str,
        has_lifetime: bool,
        resolved: &prettify::ResolvedImports,
    ) -> (Option<TokenStream>, TokenStream) {
        let lex_doc = self.corpus.get(nsid).expect("nsid exists in corpus");

        // Generate shared function name from NSID (use sanitize_name for proper handling)
        let shared_fn_name = format!("lexicon_doc_{}", utils::sanitize_name(nsid));
        let shared_fn_ident = syn::Ident::new(&shared_fn_name, proc_macro2::Span::call_site());

        // Check if we need to generate the shared function
        let mut generated = self.generated_shared_docs.borrow_mut();
        let shared_fn = if !generated.contains(nsid) {
            generated.insert(nsid.to_string());
            // Codegen from JSON doesn't have union_fields (those are for Rust -> lexicon derive).
            use crate::derive_impl::doc_to_tokens::{DocPaths, doc_to_tokens_with_paths};
            let doc_paths = match self.mode {
                prettify::CodegenMode::Pretty => DocPaths::short(),
                prettify::CodegenMode::Macro => DocPaths::qualified(),
            };
            let scoped_imports = doc_paths.scoped_imports();
            let doc_literal =
                doc_to_tokens_with_paths(lex_doc, &std::collections::BTreeMap::new(), &doc_paths);
            let lexicon_doc_path =
                resolved.external_type_tokens(&prettify::ExternalImport::LexiconDoc);
            Some(quote! {
                fn #shared_fn_ident() -> #lexicon_doc_path<'static> {
                    #scoped_imports
                    #doc_literal
                }
            })
        } else {
            None
        };

        // Generate lightweight trait impl that calls shared function.
        let type_ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());
        let bosstr_tok = resolved.external_type_tokens(&prettify::ExternalImport::BosStr);
        let (impl_generics, type_generics) = if has_lifetime {
            (quote! { <S: #bosstr_tok> }, quote! { <S> })
        } else {
            (quote! {}, quote! {})
        };

        // Extract validation checks for this specific def.
        let validation_checks =
            schema_impl::extract_validation_checks(self.corpus, lex_doc, def_name);
        let validation_code = crate::derive_impl::doc_to_tokens::validations_to_tokens_resolved(
            &validation_checks,
            Some(resolved),
        );

        let constraint_error_type =
            resolved.external_type_tokens(&prettify::ExternalImport::ConstraintError);
        let schema_path = resolved.external_type_tokens(&prettify::ExternalImport::LexiconSchema);
        let lexicon_doc_path = resolved.external_type_tokens(&prettify::ExternalImport::LexiconDoc);

        let trait_impl = quote! {
            impl #impl_generics #schema_path for #type_ident #type_generics {
                fn nsid() -> &'static str {
                    #nsid
                }

                fn def_name() -> &'static str {
                    #def_name
                }

                fn lexicon_doc() -> #lexicon_doc_path<'static> {
                    #shared_fn_ident()
                }

                fn validate(&self) -> Result<(), #constraint_error_type> {
                    #validation_code
                }
            }
        };

        (shared_fn, trait_impl)
    }

    /// Generate code for a lexicon def
    pub fn generate_def(
        &self,
        nsid: &str,
        def_name: &str,
        def: &LexUserType<'static>,
        resolved: &prettify::ResolvedImports,
    ) -> Result<GeneratedCode> {
        match def {
            LexUserType::Record(record) => self.generate_record(nsid, def_name, record, resolved),
            LexUserType::Object(obj) => self.generate_object(nsid, def_name, obj, resolved),
            LexUserType::XrpcQuery(query) => self.generate_query(nsid, def_name, query, resolved),
            LexUserType::XrpcProcedure(proc) => {
                self.generate_procedure(nsid, def_name, proc, resolved)
            }
            LexUserType::Token(token) => {
                // Token types are marker structs that can be used as union refs
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let doc = self.generate_doc_comment(token.description.as_ref());

                // Token name for Display impl (just the def name, not the full ref)
                let token_name = def_name;

                let derive_attr = resolved.derive_standard_with(quote! { Hash });
                let tokens = quote! {
                    #doc
                    #derive_attr
                    pub struct #ident;

                    impl core::fmt::Display for #ident {
                        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                            write!(f, #token_name)
                        }
                    }
                };
                Ok(GeneratedCode::type_only(tokens))
            }
            LexUserType::String(s) if s.known_values.is_some() => {
                self.generate_known_values_enum(nsid, def_name, s, resolved)
            }
            LexUserType::String(s) => {
                // Plain string type alias
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let rust_type = self.string_to_rust_type(s, resolved);
                let doc = self.generate_doc_comment(s.description.as_ref());
                let needs_param = self.string_needs_type_param(s);
                let default_str_path =
                    resolved.external_type_tokens(&prettify::ExternalImport::DefaultStr);
                let tokens = if needs_param {
                    quote! {
                        #doc
                        pub type #ident<S = #default_str_path> = #rust_type;
                    }
                } else {
                    quote! {
                        #doc
                        pub type #ident = #rust_type;
                    }
                };
                Ok(GeneratedCode::type_only(tokens))
            }
            LexUserType::Integer(i) if i.r#enum.is_some() => {
                self.generate_integer_enum(nsid, def_name, i)
            }
            LexUserType::Array(array) => {
                // Top-level array becomes type alias to Vec<ItemType>
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let doc = self.generate_doc_comment(array.description.as_ref());
                let needs_lifetime = self.array_item_needs_type_param(&array.items);

                // Check if items are a union - if so, generate the union enum first
                if let LexArrayItem::Union(union) = &array.items {
                    let union_name = format!("{}Item", type_name);
                    let refs: Vec<_> = union.refs.iter().cloned().collect();
                    let union_generated = self.generate_union(
                        nsid,
                        &union_name,
                        &refs,
                        None,
                        union.closed,
                        resolved,
                    )?;

                    let union_ident = syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                    let union_tokens = union_generated.into_tokens();
                    let default_str_path =
                        resolved.external_type_tokens(&prettify::ExternalImport::DefaultStr);
                    let type_alias = if needs_lifetime {
                        quote! {
                            #doc
                            pub type #ident<S = #default_str_path> = Vec<#union_ident<S>>;
                        }
                    } else {
                        quote! {
                            #doc
                            pub type #ident = Vec<#union_ident>;
                        }
                    };

                    let type_defs = quote! {
                        #union_tokens

                        #type_alias
                    };
                    Ok(GeneratedCode::type_only(type_defs))
                } else {
                    // Regular array item type
                    let item_type = self.array_item_to_rust_type(nsid, &array.items, resolved)?;
                    let default_str_path =
                        resolved.external_type_tokens(&prettify::ExternalImport::DefaultStr);
                    let tokens = if needs_lifetime {
                        quote! {
                            #doc
                            pub type #ident<S = #default_str_path> = Vec<#item_type>;
                        }
                    } else {
                        quote! {
                            #doc
                            pub type #ident = Vec<#item_type>;
                        }
                    };
                    Ok(GeneratedCode::type_only(tokens))
                }
            }
            LexUserType::Boolean(_)
            | LexUserType::Integer(_)
            | LexUserType::Bytes(_)
            | LexUserType::CidLink(_)
            | LexUserType::Unknown(_) => {
                // These are rarely top-level defs, but if they are, make type aliases
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let (rust_type, needs_lifetime) = match def {
                    LexUserType::Boolean(_) => (quote! { bool }, false),
                    LexUserType::Integer(_) => (quote! { i64 }, false),
                    LexUserType::Bytes(_) => (
                        resolved.external_type_tokens(&prettify::ExternalImport::Bytes),
                        false,
                    ),
                    LexUserType::CidLink(_) => {
                        (resolved.type_tokens(&prettify::CommonType::CidLink), true)
                    }
                    LexUserType::Unknown(_) => {
                        (resolved.type_tokens(&prettify::CommonType::Data), true)
                    }
                    _ => unreachable!(),
                };
                let default_str_path =
                    resolved.external_type_tokens(&prettify::ExternalImport::DefaultStr);
                let tokens = if needs_lifetime {
                    quote! {
                        pub type #ident<S = #default_str_path> = #rust_type;
                    }
                } else {
                    quote! {
                        pub type #ident = #rust_type;
                    }
                };
                Ok(GeneratedCode::type_only(tokens))
            }
            LexUserType::Blob(_) => Err(CodegenError::unsupported(
                format!("top-level def type {:?}", def),
                nsid,
                None::<String>,
            )),
            LexUserType::XrpcSubscription(sub) => {
                // Track this file as containing a subscription
                let file_path = self.nsid_to_file_path(nsid);
                self.subscription_files.borrow_mut().insert(file_path);
                self.generate_subscription(nsid, def_name, sub, resolved)
            }
            LexUserType::Union(union) => {
                // Top-level union generates an enum
                let type_name = self.def_to_type_name(nsid, def_name);
                let refs: Vec<_> = union.refs.iter().cloned().collect();
                // Union already returns GeneratedCode, so just forward it
                self.generate_union(
                    nsid,
                    &type_name,
                    &refs,
                    union.description.as_ref().map(|d| d.as_ref()),
                    union.closed,
                    resolved,
                )
            }
            LexUserType::PermissionSet(_perm_set) => {
                // Permission sets are consumed at runtime by the permission set
                // resolver (Phase 5), not generated as code.
                Ok(GeneratedCode::default())
            }
            LexUserType::Space(space) => self.generate_space(nsid, def_name, space),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_record() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("app.bsky.feed.post").expect("get post");
        let def = doc.defs.get("main").expect("get main def");

        let resolved = codegen.default_resolved_imports();
        let generated = codegen
            .generate_def("app.bsky.feed.post", "main", def, &resolved)
            .expect("generate");
        let tokens = generated.into_tokens();

        // Format and print for inspection
        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check basic structure
        assert!(formatted.contains("struct Post"));
        assert!(formatted.contains("pub text"));
        // String fields use the bare type parameter S (CowStr becomes S in BOS mode).
        assert!(
            formatted.contains(": S,"),
            "expected bare S for string fields, got:\n{}",
            formatted
        );
    }

    #[test]
    fn test_generate_union() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Create a union with embed types
        let refs = vec![
            "app.bsky.embed.images".into(),
            "app.bsky.embed.video".into(),
            "app.bsky.embed.external".into(),
        ];

        let resolved = codegen.default_resolved_imports();
        let generated = codegen
            .generate_union(
                "app.bsky.feed.post",
                "RecordEmbed",
                &refs,
                Some("Post embed union"),
                None,
                &resolved,
            )
            .expect("generate union");
        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("enum RecordEmbed"));
        assert!(formatted.contains("Images"));
        assert!(formatted.contains("Video"));
        assert!(formatted.contains("External"));
        assert!(formatted.contains("tag = \"$type\""));
        assert!(formatted.contains("#[jacquard_derive::open_union]"));
    }

    #[test]
    fn test_generate_query() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("app.bsky.feed.getAuthorFeed")
            .expect("get getAuthorFeed");
        let def = doc.defs.get("main").expect("get main def");

        let resolved = codegen.default_resolved_imports();
        let generated = codegen
            .generate_def("app.bsky.feed.getAuthorFeed", "main", def, &resolved)
            .expect("generate");
        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("struct GetAuthorFeed"));
        assert!(formatted.contains("struct GetAuthorFeedOutput"));
        assert!(formatted.contains("enum GetAuthorFeedError"));
        assert!(formatted.contains("pub actor"));
        assert!(formatted.contains("pub limit"));
        assert!(formatted.contains("pub cursor"));
        assert!(formatted.contains("pub feed"));
        assert!(formatted.contains("BlockedActor"));
        assert!(formatted.contains("BlockedByActor"));
    }

    #[test]
    fn test_generate_known_values_enum() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("com.atproto.label.defs")
            .expect("get label defs");
        let def = doc.defs.get("labelValue").expect("get labelValue def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("com.atproto.label.defs", "labelValue", def, &resolved)
            .expect("generate");
        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("enum LabelValue"));
        assert!(formatted.contains("Hide"));
        assert!(formatted.contains("NoPromote"));
        assert!(formatted.contains("Warn"));
        assert!(formatted.contains("DmcaViolation"));
        // The catch-all variant uses the bare S type parameter.
        assert!(
            formatted.contains("Other(S)"),
            "expected Other(S) variant, got:\n{}",
            formatted
        );
        // from_value replaces the old From<&str> impl.
        assert!(
            formatted.contains("fn from_value"),
            "expected from_value method, got:\n{}",
            formatted
        );
        assert!(formatted.contains("fn as_str(&self)"));
    }

    #[test]
    fn test_nsid_to_file_path() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        // Regular paths
        assert_eq!(
            codegen.nsid_to_file_path("app.bsky.feed.post"),
            std::path::PathBuf::from("app_bsky/feed/post.rs")
        );

        assert_eq!(
            codegen.nsid_to_file_path("app.bsky.feed.getAuthorFeed"),
            std::path::PathBuf::from("app_bsky/feed/get_author_feed.rs")
        );

        // Defs paths - should go in parent
        assert_eq!(
            codegen.nsid_to_file_path("com.atproto.label.defs"),
            std::path::PathBuf::from("com_atproto/label.rs")
        );
    }

    #[test]
    fn test_write_to_disk() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "test_generated");

        let tmp_dir =
            tempfile::tempdir().expect("should be able to create temp directory for output");
        let output_dir = std::path::PathBuf::from(tmp_dir.path());

        // Clean up any previous test output
        let _ = std::fs::remove_dir_all(&output_dir);

        // Generate and write
        codegen.write_to_disk(&output_dir).expect("write to disk");

        // Verify some files were created
        assert!(output_dir.join("app_bsky/feed/post.rs").exists());
        assert!(output_dir.join("app_bsky/feed/get_author_feed.rs").exists());
        assert!(output_dir.join("com_atproto/label.rs").exists());

        // Verify module files were created
        assert!(output_dir.join("lib.rs").exists());
        assert!(output_dir.join("app_bsky.rs").exists());

        // Read and verify post.rs contains expected content
        let post_content = std::fs::read_to_string(output_dir.join("app_bsky/feed/post.rs"))
            .expect("read post.rs");
        assert!(post_content.contains("pub struct Post"));
        assert!(post_content.contains("jacquard_common"));
    }

    #[test]
    fn test_generate_procedure() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("com.atproto.repo.createRecord")
            .expect("get createRecord");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("com.atproto.repo.createRecord", "main", def, &resolved)
            .expect("generate");
        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure - procedures have input, output, and error types
        assert!(formatted.contains("struct CreateRecord"));
        assert!(formatted.contains("struct CreateRecordOutput"));
        assert!(formatted.contains("enum CreateRecordError"));
        // Check input fields
        assert!(formatted.contains("pub repo"));
        assert!(formatted.contains("pub collection"));
        assert!(formatted.contains("pub record"));
        // Check output fields
        assert!(formatted.contains("pub uri"));
        assert!(formatted.contains("pub cid"));
        // Check error variants
        assert!(formatted.contains("InvalidSwap"));
        assert!(formatted.contains("InvalidRecord"));
    }

    #[test]
    fn test_generate_subscription() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("com.atproto.sync.subscribeRepos")
            .expect("get subscribeRepos");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("com.atproto.sync.subscribeRepos", "main", def, &resolved)
            .expect("generate");
        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check subscription structure
        assert!(formatted.contains("struct SubscribeRepos"));
        assert!(formatted.contains("enum SubscribeReposMessage"));
        // Check message union variants
        assert!(formatted.contains("Commit"));
        assert!(formatted.contains("Identity"));
        assert!(formatted.contains("Account"));
    }

    // #[test]
    // fn test_generate_token_type() {
    //     let corpus =
    //         LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
    //     let codegen = CodeGenerator::new(&corpus, "jacquard_api");

    //     let doc = corpus.get("app.bsky.embed.images").expect("get images");
    //     let def = doc.defs.get("viewImage").expect("get viewImage def");

    //     let tokens = codegen
    //         .generate_def("app.bsky.embed.images", "viewImage", def, &resolved)
    //         .expect("generate");

    //     let file: syn::File = syn::parse2(tokens).expect("parse tokens");
    //     let formatted = prettyplease::unparse(&file);
    //     println!("\n{}\n", formatted);

    //     // Token types are unit structs
    //     assert!(formatted.contains("struct ViewImage"));
    //     // Should have Display implementation
    //     assert!(formatted.contains("impl std::fmt::Display"));
    // }

    #[test]
    fn test_generate_array_types() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("test.array.types").expect("get array types");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("test.array.types", "main", def, &resolved)
            .expect("generate");

        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check different array item types
        assert!(formatted.contains("simple_strings"));
        assert!(formatted.contains("Vec<"));
        // Union array items should generate enum
        assert!(formatted.contains("union_items"));
        // Ref array items
        assert!(formatted.contains("ref_items"));
        // CID link arrays
        assert!(formatted.contains("cid_links"));
    }

    #[test]
    fn test_generate_binary_types() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("test.binary.types").expect("get binary types");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("test.binary.types", "main", def, &resolved)
            .expect("generate");

        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check binary field types
        assert!(formatted.contains("pub cid"));
        assert!(formatted.contains("CidLink") || formatted.contains("types::cid"));
        assert!(formatted.contains("pub data"));
        assert!(formatted.contains("Bytes"));
        assert!(formatted.contains("pub avatar"));
        assert!(formatted.contains("BlobRef") || formatted.contains("types::blob"));
    }

    #[test]
    fn test_generate_empty_object() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("test.empty.object").expect("get empty object");
        let def = doc.defs.get("emptyDef").expect("get emptyDef");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("test.empty.object", "emptyDef", def, &resolved)
            .expect("generate");

        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Empty objects should generate type alias to Data<S>.
        assert!(formatted.contains("type EmptyDef") || formatted.contains("Data<S>"));
    }

    #[test]
    fn test_generate_multi_def_lexicon() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("pub.leaflet.poll.definition")
            .expect("get poll definition");

        // Test main def
        let main_def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();
        let main_generated = codegen
            .generate_def("pub.leaflet.poll.definition", "main", main_def, &resolved)
            .expect("generate main");
        let main_tokens = main_generated.into_tokens();
        let main_file: syn::File = syn::parse2(main_tokens).expect("parse main tokens");
        let main_formatted = prettyplease::unparse(&main_file);
        println!("\nMain:\n{}\n", main_formatted);
        assert!(main_formatted.contains("struct Definition"));
        assert!(main_formatted.contains("pub question"));
        assert!(main_formatted.contains("pub options"));

        // Test option fragment
        let option_def = doc.defs.get("option").expect("get option def");
        let option_generated = codegen
            .generate_def(
                "pub.leaflet.poll.definition",
                "option",
                option_def,
                &resolved,
            )
            .expect("generate option");
        let option_tokens = option_generated.into_tokens();
        let option_file: syn::File = syn::parse2(option_tokens).expect("parse option tokens");
        let option_formatted = prettyplease::unparse(&option_file);
        println!("\nOption:\n{}\n", option_formatted);
        assert!(option_formatted.contains("struct DefinitionOption"));
        assert!(option_formatted.contains("pub text"));

        // Test vote fragment
        let vote_def = doc.defs.get("vote").expect("get vote def");
        let vote_generated = codegen
            .generate_def("pub.leaflet.poll.definition", "vote", vote_def, &resolved)
            .expect("generate vote");
        let vote_tokens = vote_generated.into_tokens();
        let vote_file: syn::File = syn::parse2(vote_tokens).expect("parse vote tokens");
        let vote_formatted = prettyplease::unparse(&vote_file);
        println!("\nVote:\n{}\n", vote_formatted);
        assert!(
            vote_formatted.contains("struct DefinitionVote")
                || vote_formatted.contains("struct Vote")
        );
        assert!(vote_formatted.contains("pub poll_ref"));
        assert!(vote_formatted.contains("pub option_index"));
    }

    #[test]
    fn test_generate_with_constraints_and_defaults() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("test.constraints.validation")
            .expect("get constraints");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("test.constraints.validation", "main", def, &resolved)
            .expect("generate");

        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check fields with constraints are generated
        assert!(formatted.contains("pub username"));
        assert!(formatted.contains("pub bio"));
        assert!(formatted.contains("pub age"));
        assert!(formatted.contains("pub enabled"));
        assert!(formatted.contains("pub tags"));
        assert!(formatted.contains("pub role"));

        // Constraints should be in docs or validation metadata
        // (exact format depends on codegen implementation)
    }

    #[test]
    fn test_local_refs_in_definitions() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus
            .get("pub.leaflet.poll.definition")
            .expect("get poll definition");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("pub.leaflet.poll.definition", "main", def, &resolved)
            .expect("generate");

        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Local ref #option should resolve to DefinitionOption type (fully qualified or local)
        assert!(
            formatted.contains("Vec<DefinitionOption")
                || formatted
                    .contains("Vec<jacquard_api::pub_leaflet::poll::definition::DefinitionOption")
        );
    }

    #[test]
    fn test_nullable_optional_properties() {
        let corpus =
            LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons").expect("load corpus");
        let codegen = CodeGenerator::new(&corpus, "jacquard_api");

        let doc = corpus.get("test.binary.types").expect("get binary types");
        let def = doc.defs.get("main").expect("get main def");
        let resolved = codegen.default_resolved_imports();

        let generated = codegen
            .generate_def("test.binary.types", "main", def, &resolved)
            .expect("generate");

        let tokens = generated.into_tokens();

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Optional fields should use Option<T>
        assert!(formatted.contains("optional_cid"));
        assert!(formatted.contains("Option<"));
    }
}
