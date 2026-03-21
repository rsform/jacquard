use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeSet;

/// Mode flag for codegen output style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CodegenMode {
    /// Fully-qualified paths everywhere, no use block. For proc-macro consumers.
    #[default]
    Macro,
    /// Short names with grouped use block. For human-readable file output.
    Pretty,
}

/// Common types from jacquard_common that appear frequently in generated code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommonType {
    // string types
    Did,
    Handle,
    AtUri,
    Nsid,
    Tid,
    Cid,
    CidLink,
    Datetime,
    Language,
    RecordKey,
    // value types
    Data,
    RawData,
    // blob
    BlobRef,
    // cow
    CowStr,
    SmolStr,
    // collection
    Collection,
    RecordError,
}

/// External crate imports (serde, bytes, jacquard_derive, jacquard_lexicon, etc.).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExternalImport {
    // serde derives
    Serialize,
    Deserialize,
    // jacquard_derive derives and attribute macros
    IntoStatic,
    LexiconAttr,   // #[jacquard_derive::lexicon] attribute macro
    OpenUnion,     // #[jacquard_derive::open_union] attribute macro
    // jacquard_lexicon types (used in LexiconSchema impls)
    LexiconSchema,
    LexiconDoc,
    ConstraintError,
    // external crate types
    Bytes,
}

/// Accumulator for types referenced during generation (without path-form decisions).
#[derive(Default, Clone, Debug)]
pub struct ImportSet {
    pub common: BTreeSet<CommonType>,
    pub lexicon_refs: BTreeSet<String>,
    pub external: BTreeSet<ExternalImport>,
}

impl ImportSet {
    pub fn merge(&mut self, other: ImportSet) {
        self.common.extend(other.common);
        self.lexicon_refs.extend(other.lexicon_refs);
        self.external.extend(other.external);
    }
}

/// Decision table built between collection and emission passes.
/// Maps each type to either a short Ident or fully-qualified form.
/// Stub in Phase 1 — always returns fully-qualified paths.
pub struct ResolvedImports {
    mode: CodegenMode,
}

impl ResolvedImports {
    pub fn macro_mode() -> Self {
        Self {
            mode: CodegenMode::Macro,
        }
    }
}

/// Structured output from a sub-generator (params struct, input struct, output struct).
/// Separates type definition tokens from companion items (default functions, builders)
/// so callers can route them to the correct buckets.
pub struct SubGeneratorOutput {
    /// The struct/enum definition itself.
    pub type_def: TokenStream,
    /// Companion serde default functions.
    pub default_fns: TokenStream,
    /// Builder module and struct, if generated.
    pub builder: TokenStream,
}

/// Structured output from a single generator invocation.
/// Four token buckets enable file-level reordering.
pub struct GeneratedCode {
    /// Primary type definitions: structs, enums, type aliases.
    pub type_defs: TokenStream,
    /// Inherent impl blocks: fn new(), fn uri(), associated functions.
    pub inherent_impls: TokenStream,
    /// Trait impl blocks: XrpcRequest, XrpcResp, Collection, LexiconSchema,
    /// plus their marker structs (bundled together).
    pub trait_impls: TokenStream,
    /// Internal/private items: builder state modules, builder structs,
    /// lexicon_doc_* functions.
    pub internals: TokenStream,
    /// Types referenced during generation (for import collection in Phase 2).
    pub imports: ImportSet,
}

impl GeneratedCode {
    /// Create an empty GeneratedCode with no tokens.
    pub fn empty() -> Self {
        Self {
            type_defs: TokenStream::new(),
            inherent_impls: TokenStream::new(),
            trait_impls: TokenStream::new(),
            internals: TokenStream::new(),
            imports: ImportSet::default(),
        }
    }

    /// Create GeneratedCode with tokens only in type_defs (for simple defs
    /// like tokens, type aliases, etc. that don't have impls or builders).
    pub fn type_only(tokens: TokenStream) -> Self {
        Self {
            type_defs: tokens,
            ..Self::empty()
        }
    }

    /// Flatten all buckets into a single TokenStream (preserving bucket order).
    /// Used when embedding nested types into a parent's type_defs bucket.
    pub fn into_tokens(self) -> TokenStream {
        let Self {
            type_defs,
            inherent_impls,
            trait_impls,
            internals,
            ..
        } = self;
        quote! {
            #type_defs
            #inherent_impls
            #trait_impls
            #internals
        }
    }
}

impl Default for GeneratedCode {
    fn default() -> Self {
        Self::empty()
    }
}

/// Assembly output for a single generated file.
#[derive(Clone)]
pub struct FileOutput {
    pub tokens: TokenStream,
    pub imports: ImportSet,
    pub nsid: Option<String>,
}

impl FileOutput {
    /// Combine multiple GeneratedCode structs (from all definitions in a file)
    /// into a single FileOutput with reordered tokens.
    ///
    /// Ordering: all type_defs, then all inherent_impls, then all trait_impls,
    /// then all internals.
    pub fn combine(items: Vec<GeneratedCode>, nsid: Option<String>) -> Self {
        let mut all_type_defs = TokenStream::new();
        let mut all_inherent_impls = TokenStream::new();
        let mut all_trait_impls = TokenStream::new();
        let mut all_internals = TokenStream::new();
        let mut merged_imports = ImportSet::default();

        for item in items {
            all_type_defs.extend(item.type_defs);
            all_inherent_impls.extend(item.inherent_impls);
            all_trait_impls.extend(item.trait_impls);
            all_internals.extend(item.internals);
            merged_imports.merge(item.imports);
        }

        let tokens = quote! {
            #all_type_defs
            #all_inherent_impls
            #all_trait_impls
            #all_internals
        };

        Self {
            tokens,
            imports: merged_imports,
            nsid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generated_code_empty() {
        let generated = GeneratedCode::empty();
        assert_eq!(generated.type_defs.to_string(), "");
        assert_eq!(generated.inherent_impls.to_string(), "");
        assert_eq!(generated.trait_impls.to_string(), "");
        assert_eq!(generated.internals.to_string(), "");
        assert!(generated.imports.common.is_empty());
        assert!(generated.imports.lexicon_refs.is_empty());
        assert!(generated.imports.external.is_empty());
    }

    #[test]
    fn test_generated_code_type_only() {
        let tokens = quote! { struct Foo; };
        let generated = GeneratedCode::type_only(tokens.clone());
        assert_eq!(generated.type_defs.to_string(), tokens.to_string());
        assert_eq!(generated.inherent_impls.to_string(), "");
        assert_eq!(generated.trait_impls.to_string(), "");
        assert_eq!(generated.internals.to_string(), "");
    }

    #[test]
    fn test_generated_code_into_tokens() {
        let type_defs = quote! { struct A; };
        let inherent_impls = quote! { impl A { } };
        let trait_impls = quote! { impl Clone for A { } };
        let internals = quote! { mod private { } };

        let generated = GeneratedCode {
            type_defs,
            inherent_impls,
            trait_impls,
            internals,
            imports: ImportSet::default(),
        };

        let flattened = generated.into_tokens().to_string();
        assert!(flattened.contains("struct A"));
        assert!(flattened.contains("impl A"));
        assert!(flattened.contains("impl Clone for A"));
        assert!(flattened.contains("mod private"));
    }

    #[test]
    fn test_file_output_combine_empty() {
        let result = FileOutput::combine(vec![], None);
        assert_eq!(result.tokens.to_string(), "");
        assert!(result.imports.common.is_empty());
        assert!(result.imports.lexicon_refs.is_empty());
        assert!(result.imports.external.is_empty());
        assert_eq!(result.nsid, None);
    }

    #[test]
    fn test_file_output_combine_reorders() {
        let gen1 = GeneratedCode {
            type_defs: quote! { struct Foo; },
            inherent_impls: quote! { impl Foo { } },
            trait_impls: quote! { impl Clone for Foo { } },
            internals: quote! { mod foo_internals { } },
            imports: ImportSet::default(),
        };

        let gen2 = GeneratedCode {
            type_defs: quote! { struct Bar; },
            inherent_impls: quote! { impl Bar { } },
            trait_impls: quote! { impl Clone for Bar { } },
            internals: quote! { mod bar_internals { } },
            imports: ImportSet::default(),
        };

        let result = FileOutput::combine(vec![gen1, gen2], Some("test.nsid".to_string()));

        let output = result.tokens.to_string();
        // Find positions to verify ordering
        let struct_foo_pos = output.find("struct Foo").expect("struct Foo not found");
        let struct_bar_pos = output.find("struct Bar").expect("struct Bar not found");
        let impl_foo_pos = output.find("impl Foo").expect("impl Foo not found");
        let impl_bar_pos = output.find("impl Bar").expect("impl Bar not found");
        let clone_foo_pos = output.find("impl Clone for Foo").expect("impl Clone for Foo not found");
        let clone_bar_pos = output.find("impl Clone for Bar").expect("impl Clone for Bar not found");
        let foo_int_pos = output.find("mod foo_internals").expect("mod foo_internals not found");
        let bar_int_pos = output.find("mod bar_internals").expect("mod bar_internals not found");

        // type_defs should all come before inherent_impls
        assert!(struct_foo_pos < impl_foo_pos);
        assert!(struct_foo_pos < impl_bar_pos);
        assert!(struct_bar_pos < impl_foo_pos);
        assert!(struct_bar_pos < impl_bar_pos);

        // inherent_impls should all come before trait_impls
        assert!(impl_foo_pos < clone_foo_pos);
        assert!(impl_foo_pos < clone_bar_pos);
        assert!(impl_bar_pos < clone_foo_pos);
        assert!(impl_bar_pos < clone_bar_pos);

        // trait_impls should all come before internals
        assert!(clone_foo_pos < foo_int_pos);
        assert!(clone_foo_pos < bar_int_pos);
        assert!(clone_bar_pos < foo_int_pos);
        assert!(clone_bar_pos < bar_int_pos);

        assert_eq!(result.nsid, Some("test.nsid".to_string()));
    }

    #[test]
    fn test_import_set_merge() {
        let mut set1 = ImportSet::default();
        set1.common.insert(CommonType::Did);
        set1.external.insert(ExternalImport::Serialize);
        set1.lexicon_refs.insert("foo.bar".to_string());

        let mut set2 = ImportSet::default();
        set2.common.insert(CommonType::Handle);
        set2.external.insert(ExternalImport::Deserialize);
        set2.lexicon_refs.insert("baz.qux".to_string());

        set1.merge(set2);

        assert!(set1.common.contains(&CommonType::Did));
        assert!(set1.common.contains(&CommonType::Handle));
        assert!(set1.external.contains(&ExternalImport::Serialize));
        assert!(set1.external.contains(&ExternalImport::Deserialize));
        assert!(set1.lexicon_refs.contains("foo.bar"));
        assert!(set1.lexicon_refs.contains("baz.qux"));
    }
}
