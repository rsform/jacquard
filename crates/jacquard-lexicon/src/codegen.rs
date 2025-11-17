use crate::corpus::LexiconCorpus;
use crate::error::{CodegenError, Result};
use crate::lexicon::{LexArrayItem, LexUserType};
use proc_macro2::TokenStream;
use quote::quote;

pub(crate) mod builder_gen;
pub(crate) mod builder_heuristics;
pub(crate) mod lifetime;
pub(crate) mod names;
pub(crate) mod nsid_utils;
pub(crate) mod output;
pub(crate) mod schema_impl;
pub(crate) mod structs;
pub(crate) mod types;
pub(crate) mod union_codegen;
pub(crate) mod utils;
pub(crate) mod xrpc;

/// Code generator for lexicon types
pub struct CodeGenerator<'c> {
    corpus: &'c LexiconCorpus,
    root_module: String,
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
            namespace_deps: std::cell::RefCell::new(std::collections::HashMap::new()),
            subscription_files: std::cell::RefCell::new(std::collections::HashSet::new()),
            generated_shared_docs: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Generate doc comment from optional description (wrapper for utils function)
    fn generate_doc_comment(&self, desc: Option<&jacquard_common::CowStr>) -> TokenStream {
        utils::generate_doc_comment(desc)
    }

    /// Generate or reference the shared lexicon_doc function for this NSID
    /// Returns (optional shared function, trait impl tokens)
    pub(crate) fn generate_schema_impl_with_shared(
        &self,
        type_name: &str,
        nsid: &str,
        def_name: &str,
        has_lifetime: bool,
    ) -> (Option<TokenStream>, TokenStream) {
        let lex_doc = self.corpus.get(nsid).expect("nsid exists in corpus");

        // Generate shared function name from NSID (use sanitize_name for proper handling)
        let shared_fn_name = format!("lexicon_doc_{}", utils::sanitize_name(nsid));
        let shared_fn_ident = syn::Ident::new(&shared_fn_name, proc_macro2::Span::call_site());

        // Check if we need to generate the shared function
        let mut generated = self.generated_shared_docs.borrow_mut();
        let shared_fn = if !generated.contains(nsid) {
            generated.insert(nsid.to_string());
            // Codegen from JSON doesn't have union_fields (those are for Rust -> lexicon derive)
            let doc_literal = crate::derive_impl::doc_to_tokens::doc_to_tokens(
                lex_doc,
                &std::collections::BTreeMap::new(),
            );
            Some(quote! {
                fn #shared_fn_ident() -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                    #doc_literal
                }
            })
        } else {
            None
        };

        // Generate lightweight trait impl that calls shared function
        let type_ident = syn::Ident::new(type_name, proc_macro2::Span::call_site());
        let (impl_generics, type_generics) = if has_lifetime {
            (quote! { <'a> }, quote! { <'a> })
        } else {
            (quote! {}, quote! {})
        };

        // Extract validation checks for this specific def
        let validation_checks = schema_impl::extract_validation_checks(lex_doc, def_name);
        let validation_code =
            crate::derive_impl::doc_to_tokens::validations_to_tokens(&validation_checks);

        let trait_impl = quote! {
            impl #impl_generics ::jacquard_lexicon::schema::LexiconSchema for #type_ident #type_generics {
                fn nsid() -> &'static str {
                    #nsid
                }

                fn def_name() -> &'static str {
                    #def_name
                }

                fn lexicon_doc() -> ::jacquard_lexicon::lexicon::LexiconDoc<'static> {
                    #shared_fn_ident()
                }

                fn validate(&self) -> ::std::result::Result<(), ::jacquard_lexicon::validation::ConstraintError> {
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
    ) -> Result<TokenStream> {
        match def {
            LexUserType::Record(record) => self.generate_record(nsid, def_name, record),
            LexUserType::Object(obj) => self.generate_object(nsid, def_name, obj),
            LexUserType::XrpcQuery(query) => self.generate_query(nsid, def_name, query),
            LexUserType::XrpcProcedure(proc) => self.generate_procedure(nsid, def_name, proc),
            LexUserType::Token(token) => {
                // Token types are marker structs that can be used as union refs
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let doc = self.generate_doc_comment(token.description.as_ref());

                // Token name for Display impl (just the def name, not the full ref)
                let token_name = def_name;

                Ok(quote! {
                    #doc
                    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, Hash, jacquard_derive::IntoStatic)]
                    pub struct #ident;

                    impl std::fmt::Display for #ident {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            write!(f, #token_name)
                        }
                    }
                })
            }
            LexUserType::String(s) if s.known_values.is_some() => {
                self.generate_known_values_enum(nsid, def_name, s)
            }
            LexUserType::String(s) => {
                // Plain string type alias
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let rust_type = self.string_to_rust_type(s);
                let doc = self.generate_doc_comment(s.description.as_ref());
                Ok(quote! {
                    #doc
                    pub type #ident<'a> = #rust_type;
                })
            }
            LexUserType::Integer(i) if i.r#enum.is_some() => {
                self.generate_integer_enum(nsid, def_name, i)
            }
            LexUserType::Array(array) => {
                // Top-level array becomes type alias to Vec<ItemType>
                let type_name = self.def_to_type_name(nsid, def_name);
                let ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
                let doc = self.generate_doc_comment(array.description.as_ref());
                let needs_lifetime = self.array_item_needs_lifetime(&array.items);

                // Check if items are a union - if so, generate the union enum first
                if let LexArrayItem::Union(union) = &array.items {
                    let union_name = format!("{}Item", type_name);
                    let refs: Vec<_> = union.refs.iter().cloned().collect();
                    let union_def =
                        self.generate_union(nsid, &union_name, &refs, None, union.closed)?;

                    let union_ident = syn::Ident::new(&union_name, proc_macro2::Span::call_site());
                    if needs_lifetime {
                        Ok(quote! {
                            #union_def

                            #doc
                            pub type #ident<'a> = Vec<#union_ident<'a>>;
                        })
                    } else {
                        Ok(quote! {
                            #union_def

                            #doc
                            pub type #ident = Vec<#union_ident>;
                        })
                    }
                } else {
                    // Regular array item type
                    let item_type = self.array_item_to_rust_type(nsid, &array.items)?;
                    if needs_lifetime {
                        Ok(quote! {
                            #doc
                            pub type #ident<'a> = Vec<#item_type>;
                        })
                    } else {
                        Ok(quote! {
                            #doc
                            pub type #ident = Vec<#item_type>;
                        })
                    }
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
                    LexUserType::Bytes(_) => (quote! { bytes::Bytes }, false),
                    LexUserType::CidLink(_) => {
                        (quote! { jacquard_common::types::cid::CidLink<'a> }, true)
                    }
                    LexUserType::Unknown(_) => {
                        (quote! { jacquard_common::types::value::Data<'a> }, true)
                    }
                    _ => unreachable!(),
                };
                if needs_lifetime {
                    Ok(quote! {
                        pub type #ident<'a> = #rust_type;
                    })
                } else {
                    Ok(quote! {
                        pub type #ident = #rust_type;
                    })
                }
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
                self.generate_subscription(nsid, def_name, sub)
            }
            LexUserType::Union(union) => {
                // Top-level union generates an enum
                let type_name = self.def_to_type_name(nsid, def_name);
                let refs: Vec<_> = union.refs.iter().cloned().collect();
                self.generate_union(
                    nsid,
                    &type_name,
                    &refs,
                    union.description.as_ref().map(|d| d.as_ref()),
                    union.closed,
                )
            }
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

        let tokens = codegen
            .generate_def("app.bsky.feed.post", "main", def)
            .expect("generate");

        // Format and print for inspection
        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check basic structure
        assert!(formatted.contains("struct Post"));
        assert!(formatted.contains("pub text"));
        assert!(formatted.contains("CowStr<'a>"));
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

        let tokens = codegen
            .generate_union(
                "app.bsky.feed.post",
                "RecordEmbed",
                &refs,
                Some("Post embed union"),
                None,
            )
            .expect("generate union");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("enum RecordEmbed"));
        assert!(formatted.contains("Images"));
        assert!(formatted.contains("Video"));
        assert!(formatted.contains("External"));
        assert!(formatted.contains("#[serde(tag = \"$type\")]"));
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

        let tokens = codegen
            .generate_def("app.bsky.feed.getAuthorFeed", "main", def)
            .expect("generate");

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

        let tokens = codegen
            .generate_def("com.atproto.label.defs", "labelValue", def)
            .expect("generate");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Check structure
        assert!(formatted.contains("enum LabelValue"));
        assert!(formatted.contains("Hide"));
        assert!(formatted.contains("NoPromote"));
        assert!(formatted.contains("Warn"));
        assert!(formatted.contains("DmcaViolation"));
        assert!(formatted.contains("Other(jacquard_common::CowStr"));
        assert!(formatted.contains("impl<'a> From<&'a str>"));
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

        let tokens = codegen
            .generate_def("com.atproto.repo.createRecord", "main", def)
            .expect("generate");

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

        let tokens = codegen
            .generate_def("com.atproto.sync.subscribeRepos", "main", def)
            .expect("generate");

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
    //         .generate_def("app.bsky.embed.images", "viewImage", def)
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

        let tokens = codegen
            .generate_def("test.array.types", "main", def)
            .expect("generate");

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

        let tokens = codegen
            .generate_def("test.binary.types", "main", def)
            .expect("generate");

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

        let tokens = codegen
            .generate_def("test.empty.object", "emptyDef", def)
            .expect("generate");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Empty objects should generate type alias to Data<'a>
        assert!(formatted.contains("type EmptyDef") || formatted.contains("Data<'a>"));
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
        let main_tokens = codegen
            .generate_def("pub.leaflet.poll.definition", "main", main_def)
            .expect("generate main");
        let main_file: syn::File = syn::parse2(main_tokens).expect("parse main tokens");
        let main_formatted = prettyplease::unparse(&main_file);
        println!("\nMain:\n{}\n", main_formatted);
        assert!(main_formatted.contains("struct Definition"));
        assert!(main_formatted.contains("pub question"));
        assert!(main_formatted.contains("pub options"));

        // Test option fragment
        let option_def = doc.defs.get("option").expect("get option def");
        let option_tokens = codegen
            .generate_def("pub.leaflet.poll.definition", "option", option_def)
            .expect("generate option");
        let option_file: syn::File = syn::parse2(option_tokens).expect("parse option tokens");
        let option_formatted = prettyplease::unparse(&option_file);
        println!("\nOption:\n{}\n", option_formatted);
        assert!(option_formatted.contains("struct DefinitionOption"));
        assert!(option_formatted.contains("pub text"));

        // Test vote fragment
        let vote_def = doc.defs.get("vote").expect("get vote def");
        let vote_tokens = codegen
            .generate_def("pub.leaflet.poll.definition", "vote", vote_def)
            .expect("generate vote");
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

        let tokens = codegen
            .generate_def("test.constraints.validation", "main", def)
            .expect("generate");

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

        let tokens = codegen
            .generate_def("pub.leaflet.poll.definition", "main", def)
            .expect("generate");

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

        let tokens = codegen
            .generate_def("test.binary.types", "main", def)
            .expect("generate");

        let file: syn::File = syn::parse2(tokens).expect("parse tokens");
        let formatted = prettyplease::unparse(&file);
        println!("\n{}\n", formatted);

        // Optional fields should use Option<T>
        assert!(formatted.contains("optional_cid"));
        assert!(formatted.contains("Option<"));
    }
}
