use proc_macro2::TokenStream;
use quote::quote;
use std::collections::{BTreeMap, BTreeSet, HashSet};

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
    // string types (jacquard_common::types::string::*)
    Did,
    Handle,
    AtUri,
    Nsid,
    Tid,
    Cid,
    Datetime,
    Language,
    RecordKey,
    Rkey,
    UriValue,
    // ident types (jacquard_common::types::ident::*)
    AtIdentifier,
    // cid types (jacquard_common::types::cid::*)
    CidLink,
    // blob types (jacquard_common::types::blob::*)
    BlobRef,
    // value types (jacquard_common::types::value::*)
    Data,
    RawData,
    // collection types (jacquard_common::types::collection::*)
    Collection,
    RecordError,
    // cow/string types (jacquard_common::*)
    CowStr,
    SmolStr,
}

impl CommonType {
    /// The short name used in Pretty mode (the final path segment).
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Did => "Did",
            Self::Handle => "Handle",
            Self::AtUri => "AtUri",
            Self::Nsid => "Nsid",
            Self::Tid => "Tid",
            Self::Cid => "Cid",
            Self::Datetime => "Datetime",
            Self::Language => "Language",
            Self::RecordKey => "RecordKey",
            Self::Rkey => "Rkey",
            Self::UriValue => "UriValue",
            Self::AtIdentifier => "AtIdentifier",
            Self::CidLink => "CidLink",
            Self::BlobRef => "BlobRef",
            Self::Data => "Data",
            Self::RawData => "RawData",
            Self::Collection => "Collection",
            Self::RecordError => "RecordError",
            Self::CowStr => "CowStr",
            Self::SmolStr => "SmolStr",
        }
    }

    /// The fully-qualified token path used in Macro mode.
    /// Returns (path_tokens, needs_lifetime).
    pub fn fully_qualified(&self) -> (&'static str, bool) {
        match self {
            Self::Did => ("jacquard_common::types::string::Did", true),
            Self::Handle => ("jacquard_common::types::string::Handle", true),
            Self::Datetime => ("jacquard_common::types::string::Datetime", false),
            Self::Language => ("jacquard_common::types::string::Language", false),
            Self::Tid => ("jacquard_common::types::string::Tid", false),
            Self::CowStr => ("jacquard_common::CowStr", true),
            Self::SmolStr => ("jacquard_common::deps::smol_str::SmolStr", false),
            Self::Data => ("jacquard_common::types::value::Data", true),
            Self::CidLink => ("jacquard_common::types::cid::CidLink", true),
            Self::BlobRef => ("jacquard_common::types::blob::BlobRef", true),
            Self::AtUri => ("jacquard_common::types::string::AtUri", true),
            Self::Nsid => ("jacquard_common::types::string::Nsid", true),
            Self::Cid => ("jacquard_common::types::string::Cid", true),
            // RecordKey is a COMPOSITE type: RecordKey<Rkey<'a>>. needs_lifetime
            // is false here because the lifetime is carried by the inner Rkey, not
            // RecordKey itself. type_tokens() must special-case this.
            Self::RecordKey => ("jacquard_common::types::string::RecordKey", false),
            Self::Rkey => ("jacquard_common::types::string::Rkey", true),
            Self::UriValue => ("jacquard_common::types::string::UriValue", true),
            Self::AtIdentifier => ("jacquard_common::types::ident::AtIdentifier", true),
            Self::Collection => ("jacquard_common::types::collection::Collection", false),
            Self::RecordError => ("jacquard_common::types::collection::RecordError", true),
            Self::RawData => ("jacquard_common::types::value::RawData", true),
        }
    }

    /// The `use` path for grouping imports (everything before the type name).
    pub fn use_path(&self) -> &'static str {
        match self {
            Self::Did
            | Self::Handle
            | Self::AtUri
            | Self::Nsid
            | Self::Tid
            | Self::Cid
            | Self::Datetime
            | Self::Language
            | Self::RecordKey
            | Self::Rkey
            | Self::UriValue => "jacquard_common::types::string",
            Self::AtIdentifier => "jacquard_common::types::ident",
            Self::CidLink => "jacquard_common::types::cid",
            Self::BlobRef => "jacquard_common::types::blob",
            Self::Data | Self::RawData => "jacquard_common::types::value",
            Self::Collection | Self::RecordError => "jacquard_common::types::collection",
            Self::CowStr => "jacquard_common",
            Self::SmolStr => "jacquard_common::deps::smol_str",
        }
    }
}

/// External crate imports (serde, bytes, jacquard_derive, jacquard_lexicon, etc.).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExternalImport {
    // serde derives
    Serialize,
    Deserialize,
    // jacquard_derive derives and attribute macros
    IntoStatic,
    LexiconAttr, // #[jacquard_derive::lexicon] attribute macro
    OpenUnion,   // #[jacquard_derive::open_union] attribute macro
    // jacquard_lexicon types (used in LexiconSchema impls)
    LexiconSchema,
    LexiconDoc,
    ConstraintError,
    // external crate types
    Bytes,
}

impl ExternalImport {
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Serialize => "Serialize",
            Self::Deserialize => "Deserialize",
            Self::IntoStatic => "IntoStatic",
            Self::LexiconAttr => "lexicon",
            Self::OpenUnion => "open_union",
            Self::Bytes => "Bytes",
            Self::LexiconSchema => "LexiconSchema",
            Self::LexiconDoc => "LexiconDoc",
            Self::ConstraintError => "ConstraintError",
        }
    }

    pub fn use_path(&self) -> &'static str {
        match self {
            Self::Serialize | Self::Deserialize => "serde",
            Self::IntoStatic | Self::LexiconAttr | Self::OpenUnion => "jacquard_derive",
            Self::Bytes => "jacquard_common::deps::bytes",
            Self::LexiconSchema => "jacquard_lexicon::schema",
            Self::LexiconDoc => "jacquard_lexicon::lexicon",
            Self::ConstraintError => "jacquard_lexicon::validation",
        }
    }
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
pub struct ResolvedImports {
    mode: CodegenMode,
    /// CommonTypes that resolved to short names (no collision).
    short: BTreeMap<CommonType, syn::Ident>,
    /// CommonTypes that must stay fully-qualified (collision or Macro mode).
    qualified: BTreeSet<CommonType>,
    /// ExternalImports that resolved to short names.
    external_short: BTreeMap<ExternalImport, syn::Ident>,
    /// Cross-namespace lexicon refs to include in use block.
    lexicon_uses: BTreeSet<String>,
    /// Local type names defined in this file, used for collision detection
    /// with standard library types like `Option`.
    local_type_names: HashSet<String>,
}

impl ResolvedImports {
    /// Build decision table from collected imports, local names, and mode.
    pub fn resolve(
        imports: &ImportSet,
        local_type_names: &HashSet<String>,
        mode: CodegenMode,
    ) -> Self {
        match mode {
            CodegenMode::Macro => {
                // Everything stays fully-qualified.
                Self {
                    mode,
                    short: BTreeMap::new(),
                    qualified: imports.common.iter().cloned().collect(),
                    external_short: BTreeMap::new(),
                    lexicon_uses: BTreeSet::new(),
                    local_type_names: local_type_names.clone(),
                }
            }
            CodegenMode::Pretty => {
                let mut short = BTreeMap::new();
                let mut qualified = BTreeSet::new();

                for ct in &imports.common {
                    let name = ct.short_name();
                    if local_type_names.contains(name) {
                        // Collision — stay fully-qualified.
                        qualified.insert(ct.clone());
                    } else {
                        let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
                        short.insert(ct.clone(), ident);
                    }
                }

                let mut external_short = BTreeMap::new();
                for ei in &imports.external {
                    let name = ei.short_name();
                    // External imports don't collide with local types
                    // (Serialize, Deserialize, IntoStatic, Bytes unlikely to clash).
                    let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
                    external_short.insert(ei.clone(), ident);
                }

                Self {
                    mode,
                    short,
                    qualified,
                    external_short,
                    lexicon_uses: imports.lexicon_refs.clone(),
                    local_type_names: local_type_names.clone(),
                }
            }
        }
    }

    /// Returns tokens for a CommonType — short name or fully-qualified.
    /// Includes `<'a>` for types that need a lifetime parameter.
    ///
    /// Callers should NEVER manually append `<'a>` after calling this method.
    ///
    /// SPECIAL CASE: `RecordKey` is a composite type `RecordKey<Rkey<'a>>`.
    /// It cannot use the generic `Type<'a>` pattern. Handle it explicitly.
    pub fn type_tokens(&self, ct: &CommonType) -> TokenStream {
        // Special case: RecordKey<Rkey<'a>> is a composed generic.
        if matches!(ct, CommonType::RecordKey) {
            let rkey_tokens = self.type_tokens(&CommonType::Rkey);
            if let Some(ident) = self.short.get(ct) {
                return quote! { #ident<#rkey_tokens> };
            } else {
                let (path_str, _) = ct.fully_qualified();
                let path: syn::Path = syn::parse_str(path_str).expect("valid path");
                return quote! { #path<#rkey_tokens> };
            }
        }

        if let Some(ident) = self.short.get(ct) {
            let needs_lifetime = ct.fully_qualified().1;
            if needs_lifetime {
                quote! { #ident<'a> }
            } else {
                quote! { #ident }
            }
        } else {
            let (path_str, needs_lifetime) = ct.fully_qualified();
            let path: syn::Path = syn::parse_str(path_str).expect("valid path");
            if needs_lifetime {
                quote! { #path<'a> }
            } else {
                quote! { #path }
            }
        }
    }

    /// Returns tokens for a CommonType path WITHOUT any lifetime parameter.
    /// Used when the caller needs to supply a different lifetime (e.g., `'static`)
    /// or when the path is used as a constructor rather than a type annotation.
    pub fn type_path(&self, ct: &CommonType) -> TokenStream {
        if let Some(ident) = self.short.get(ct) {
            quote! { #ident }
        } else {
            let (path_str, _) = ct.fully_qualified();
            let path: syn::Path = syn::parse_str(path_str).expect("valid path");
            quote! { #path }
        }
    }

    /// Returns a string path for use in serde attribute string literals.
    /// Short form if imported, fully-qualified if collision or Macro mode.
    pub fn serde_path(&self, ct: &CommonType) -> String {
        if self.short.contains_key(ct) {
            ct.short_name().to_string()
        } else {
            ct.fully_qualified().0.to_string()
        }
    }

    /// Returns the correct path for `Option::is_none` in serde attributes.
    /// In Pretty mode: `"Option::is_none"` unless a local type named `Option`
    /// exists, in which case we fall back to the fully-qualified path.
    /// In Macro mode: always fully-qualified `core::option::Option::is_none`.
    pub fn option_is_none_path(&self) -> &'static str {
        match self.mode {
            CodegenMode::Pretty if !self.local_type_names.contains("Option") => "Option::is_none",
            _ => "core::option::Option::is_none",
        }
    }

    /// Standard derive attribute for most types.
    pub fn derive_standard(&self) -> TokenStream {
        let ser = self.external_path(&ExternalImport::Serialize);
        let de = self.external_path(&ExternalImport::Deserialize);
        let into_static = self.external_path(&ExternalImport::IntoStatic);
        quote! {
            #[derive(#ser, #de, Debug, Clone, PartialEq, Eq, #into_static)]
        }
    }

    /// Standard derive attribute with additional derives appended.
    /// Used for types that need Default, Hash, Copy, etc.
    pub fn derive_standard_with(&self, extra: TokenStream) -> TokenStream {
        let ser = self.external_path(&ExternalImport::Serialize);
        let de = self.external_path(&ExternalImport::Deserialize);
        let into_static = self.external_path(&ExternalImport::IntoStatic);
        quote! {
            #[derive(#ser, #de, Debug, Clone, PartialEq, Eq, #into_static, #extra)]
        }
    }

    /// Derive attribute for error enums (adds thiserror::Error, miette::Diagnostic).
    pub fn derive_error(&self) -> TokenStream {
        let ser = self.external_path(&ExternalImport::Serialize);
        let de = self.external_path(&ExternalImport::Deserialize);
        let into_static = self.external_path(&ExternalImport::IntoStatic);
        quote! {
            #[derive(#ser, #de, Debug, Clone, PartialEq, Eq, thiserror::Error, miette::Diagnostic, #into_static)]
        }
    }

    /// Returns the attribute tokens for an external import (e.g., `#[jacquard_derive::lexicon]`).
    /// Uses short form in Pretty mode if available.
    pub fn attribute_tokens(&self, ei: &ExternalImport) -> TokenStream {
        let path = self.external_path(ei);
        quote! { #[#path] }
    }

    /// Returns the correct path for an external import (either short or fully-qualified).
    fn external_path(&self, ei: &ExternalImport) -> TokenStream {
        if let Some(ident) = self.external_short.get(ei) {
            quote! { #ident }
        } else {
            let path_str = match ei {
                ExternalImport::Serialize => "serde::Serialize",
                ExternalImport::Deserialize => "serde::Deserialize",
                ExternalImport::IntoStatic => "jacquard_derive::IntoStatic",
                ExternalImport::Bytes => "jacquard_common::deps::bytes::Bytes",
                ExternalImport::LexiconAttr => "jacquard_derive::lexicon",
                ExternalImport::OpenUnion => "jacquard_derive::open_union",
                ExternalImport::LexiconSchema => "jacquard_lexicon::schema::LexiconSchema",
                ExternalImport::LexiconDoc => "jacquard_lexicon::lexicon::LexiconDoc",
                ExternalImport::ConstraintError => "jacquard_lexicon::validation::ConstraintError",
            };
            let path: syn::Path = syn::parse_str(path_str).expect("valid path");
            quote! { #path }
        }
    }

    /// Returns tokens for an external type import (short or fully-qualified).
    pub fn external_type_tokens(&self, ei: &ExternalImport) -> TokenStream {
        self.external_path(ei)
    }

    /// Produces the grouped `use` block for the top of the file.
    pub fn to_use_block(&self) -> TokenStream {
        if self.mode == CodegenMode::Macro {
            // Macro mode: no use block.
            return TokenStream::new();
        }

        // Group types by use_path.
        let mut grouped: BTreeMap<&'static str, Vec<&syn::Ident>> = BTreeMap::new();
        for (ct, ident) in &self.short {
            let path = ct.use_path();
            grouped.entry(path).or_default().push(ident);
        }

        // Group external imports by use_path.
        for (ei, ident) in &self.external_short {
            let path = ei.use_path();
            grouped.entry(path).or_default().push(ident);
        }

        // Generate use statements.
        let mut tokens = TokenStream::new();
        for (path_str, idents) in grouped {
            let path: syn::Path = syn::parse_str(path_str).expect("invalid use_path");
            tokens.extend(quote! {
                use #path::{#(#idents),*};
            });
        }

        tokens
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
    pub fn combine(
        items: Vec<GeneratedCode>,
        nsid: Option<String>,
        resolved: &ResolvedImports,
    ) -> Self {
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

        let use_block = resolved.to_use_block();
        let tokens = quote! {
            #use_block
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
        let resolved = ResolvedImports::resolve(
            &ImportSet::default(),
            &std::collections::HashSet::new(),
            CodegenMode::Macro,
        );
        let result = FileOutput::combine(vec![], None, &resolved);
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

        let resolved = ResolvedImports::resolve(
            &ImportSet::default(),
            &std::collections::HashSet::new(),
            CodegenMode::Macro,
        );
        let result =
            FileOutput::combine(vec![gen1, gen2], Some("test.nsid".to_string()), &resolved);

        let output = result.tokens.to_string();
        // Find positions to verify ordering
        let struct_foo_pos = output.find("struct Foo").expect("struct Foo not found");
        let struct_bar_pos = output.find("struct Bar").expect("struct Bar not found");
        let impl_foo_pos = output.find("impl Foo").expect("impl Foo not found");
        let impl_bar_pos = output.find("impl Bar").expect("impl Bar not found");
        let clone_foo_pos = output
            .find("impl Clone for Foo")
            .expect("impl Clone for Foo not found");
        let clone_bar_pos = output
            .find("impl Clone for Bar")
            .expect("impl Clone for Bar not found");
        let foo_int_pos = output
            .find("mod foo_internals")
            .expect("mod foo_internals not found");
        let bar_int_pos = output
            .find("mod bar_internals")
            .expect("mod bar_internals not found");

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

    #[test]
    fn test_common_type_short_names() {
        assert_eq!(CommonType::Did.short_name(), "Did");
        assert_eq!(CommonType::Handle.short_name(), "Handle");
        assert_eq!(CommonType::AtUri.short_name(), "AtUri");
        assert_eq!(CommonType::Nsid.short_name(), "Nsid");
        assert_eq!(CommonType::Tid.short_name(), "Tid");
        assert_eq!(CommonType::Cid.short_name(), "Cid");
        assert_eq!(CommonType::Datetime.short_name(), "Datetime");
        assert_eq!(CommonType::Language.short_name(), "Language");
        assert_eq!(CommonType::RecordKey.short_name(), "RecordKey");
        assert_eq!(CommonType::Rkey.short_name(), "Rkey");
        assert_eq!(CommonType::UriValue.short_name(), "UriValue");
        assert_eq!(CommonType::AtIdentifier.short_name(), "AtIdentifier");
        assert_eq!(CommonType::CidLink.short_name(), "CidLink");
        assert_eq!(CommonType::BlobRef.short_name(), "BlobRef");
        assert_eq!(CommonType::Data.short_name(), "Data");
        assert_eq!(CommonType::RawData.short_name(), "RawData");
        assert_eq!(CommonType::Collection.short_name(), "Collection");
        assert_eq!(CommonType::RecordError.short_name(), "RecordError");
        assert_eq!(CommonType::CowStr.short_name(), "CowStr");
        assert_eq!(CommonType::SmolStr.short_name(), "SmolStr");
    }

    #[test]
    fn test_common_type_fully_qualified() {
        let (path, needs_lifetime) = CommonType::Did.fully_qualified();
        assert_eq!(path, "jacquard_common::types::string::Did");
        assert!(needs_lifetime);

        let (path, needs_lifetime) = CommonType::Datetime.fully_qualified();
        assert_eq!(path, "jacquard_common::types::string::Datetime");
        assert!(!needs_lifetime);

        let (path, needs_lifetime) = CommonType::CowStr.fully_qualified();
        assert_eq!(path, "jacquard_common::CowStr");
        assert!(needs_lifetime);

        let (path, needs_lifetime) = CommonType::SmolStr.fully_qualified();
        assert_eq!(path, "jacquard_common::deps::smol_str::SmolStr");
        assert!(!needs_lifetime);

        let (path, needs_lifetime) = CommonType::Data.fully_qualified();
        assert_eq!(path, "jacquard_common::types::value::Data");
        assert!(needs_lifetime);

        let (path, needs_lifetime) = CommonType::Collection.fully_qualified();
        assert_eq!(path, "jacquard_common::types::collection::Collection");
        assert!(!needs_lifetime);

        let (path, needs_lifetime) = CommonType::RecordError.fully_qualified();
        assert_eq!(path, "jacquard_common::types::collection::RecordError");
        assert!(needs_lifetime);

        // Test RecordKey special case (composite type)
        let (path, needs_lifetime) = CommonType::RecordKey.fully_qualified();
        assert_eq!(path, "jacquard_common::types::string::RecordKey");
        assert!(!needs_lifetime);
    }

    #[test]
    fn test_common_type_use_path_grouping() {
        // All string types should share same use_path
        assert_eq!(CommonType::Did.use_path(), "jacquard_common::types::string");
        assert_eq!(
            CommonType::Handle.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(
            CommonType::AtUri.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(
            CommonType::Nsid.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(CommonType::Tid.use_path(), "jacquard_common::types::string");
        assert_eq!(CommonType::Cid.use_path(), "jacquard_common::types::string");
        assert_eq!(
            CommonType::Datetime.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(
            CommonType::Language.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(
            CommonType::RecordKey.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(
            CommonType::Rkey.use_path(),
            "jacquard_common::types::string"
        );
        assert_eq!(
            CommonType::UriValue.use_path(),
            "jacquard_common::types::string"
        );

        // Value types
        assert_eq!(CommonType::Data.use_path(), "jacquard_common::types::value");
        assert_eq!(
            CommonType::RawData.use_path(),
            "jacquard_common::types::value"
        );

        // Collection types
        assert_eq!(
            CommonType::Collection.use_path(),
            "jacquard_common::types::collection"
        );
        assert_eq!(
            CommonType::RecordError.use_path(),
            "jacquard_common::types::collection"
        );

        // Other types
        assert_eq!(
            CommonType::AtIdentifier.use_path(),
            "jacquard_common::types::ident"
        );
        assert_eq!(
            CommonType::CidLink.use_path(),
            "jacquard_common::types::cid"
        );
        assert_eq!(
            CommonType::BlobRef.use_path(),
            "jacquard_common::types::blob"
        );
        assert_eq!(CommonType::CowStr.use_path(), "jacquard_common");
        assert_eq!(
            CommonType::SmolStr.use_path(),
            "jacquard_common::deps::smol_str"
        );
    }

    #[test]
    fn test_external_import_short_names() {
        assert_eq!(ExternalImport::Serialize.short_name(), "Serialize");
        assert_eq!(ExternalImport::Deserialize.short_name(), "Deserialize");
        assert_eq!(ExternalImport::IntoStatic.short_name(), "IntoStatic");
        assert_eq!(ExternalImport::LexiconAttr.short_name(), "lexicon");
        assert_eq!(ExternalImport::OpenUnion.short_name(), "open_union");
        assert_eq!(ExternalImport::Bytes.short_name(), "Bytes");
        assert_eq!(ExternalImport::LexiconSchema.short_name(), "LexiconSchema");
        assert_eq!(ExternalImport::LexiconDoc.short_name(), "LexiconDoc");
        assert_eq!(
            ExternalImport::ConstraintError.short_name(),
            "ConstraintError"
        );
    }

    #[test]
    fn test_external_import_use_paths() {
        assert_eq!(ExternalImport::Serialize.use_path(), "serde");
        assert_eq!(ExternalImport::Deserialize.use_path(), "serde");
        assert_eq!(ExternalImport::IntoStatic.use_path(), "jacquard_derive");
        assert_eq!(ExternalImport::LexiconAttr.use_path(), "jacquard_derive");
        assert_eq!(ExternalImport::OpenUnion.use_path(), "jacquard_derive");
        assert_eq!(
            ExternalImport::Bytes.use_path(),
            "jacquard_common::deps::bytes"
        );
        assert_eq!(
            ExternalImport::LexiconSchema.use_path(),
            "jacquard_lexicon::schema"
        );
        assert_eq!(
            ExternalImport::LexiconDoc.use_path(),
            "jacquard_lexicon::lexicon"
        );
        assert_eq!(
            ExternalImport::ConstraintError.use_path(),
            "jacquard_lexicon::validation"
        );
    }

    #[test]
    fn test_resolved_imports_macro_mode() {
        // AC4.4: Macro mode should fully-qualify everything with empty use block
        let mut imports = ImportSet::default();
        imports.common.insert(CommonType::Did);
        imports.common.insert(CommonType::Handle);
        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);

        let local_names = HashSet::new();
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Macro);

        // In Macro mode, nothing goes into short maps
        assert!(resolved.short.is_empty());
        assert_eq!(resolved.qualified.len(), 2);
        assert!(resolved.qualified.contains(&CommonType::Did));
        assert!(resolved.qualified.contains(&CommonType::Handle));
        assert!(resolved.external_short.is_empty());

        // Use block should be empty in Macro mode
        let use_block = resolved.to_use_block();
        assert_eq!(use_block.to_string(), "");
    }

    #[test]
    fn test_resolved_imports_pretty_no_collisions() {
        // AC4.1: Pretty mode, no collisions → all types get short names, use block
        let mut imports = ImportSet::default();
        imports.common.insert(CommonType::Did);
        imports.common.insert(CommonType::Handle);
        imports.common.insert(CommonType::Data);
        imports.external.insert(ExternalImport::Serialize);
        imports.external.insert(ExternalImport::Deserialize);

        let local_names = HashSet::new();
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);

        // All should be in short maps
        assert_eq!(resolved.short.len(), 3);
        assert!(resolved.qualified.is_empty());
        assert_eq!(resolved.external_short.len(), 2);

        // Use block should contain the types (may have spaces or formatting variations)
        let use_block = resolved.to_use_block().to_string();
        let normalized = use_block.replace(" ", "").replace("\n", "");
        assert!(normalized.contains("jacquard_common::types::string"));
        assert!(normalized.contains("Did"));
        assert!(normalized.contains("Handle"));
        assert!(normalized.contains("jacquard_common::types::value"));
        assert!(normalized.contains("Data"));
        assert!(normalized.contains("serde"));
        assert!(normalized.contains("Serialize"));
        assert!(normalized.contains("Deserialize"));
    }

    #[test]
    fn test_resolved_imports_pretty_with_collision() {
        // AC4.2: Pretty mode, local type "Did" defined → Did stays qualified
        let mut imports = ImportSet::default();
        imports.common.insert(CommonType::Did);
        imports.common.insert(CommonType::Handle);
        imports.common.insert(CommonType::Data);
        imports.external.insert(ExternalImport::Serialize);

        let mut local_names = HashSet::new();
        local_names.insert("Did".to_string());

        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);

        // Did should be in qualified, others in short
        assert_eq!(resolved.short.len(), 2);
        assert!(resolved.short.contains_key(&CommonType::Handle));
        assert!(resolved.short.contains_key(&CommonType::Data));
        assert_eq!(resolved.qualified.len(), 1);
        assert!(resolved.qualified.contains(&CommonType::Did));

        // Use block should exclude Did
        let use_block = resolved.to_use_block().to_string();
        assert!(use_block.contains("Handle"));
        assert!(use_block.contains("Data"));
        // Did should NOT appear in the use block
        assert!(!use_block.contains("Did,") && !use_block.contains("Did}"));
    }

    #[test]
    fn test_resolved_imports_collection_collision() {
        // AC4.3: Pretty mode, local type "Collection" defined
        let mut imports = ImportSet::default();
        imports.common.insert(CommonType::Did);
        imports.common.insert(CommonType::Collection);
        imports.common.insert(CommonType::RecordError);

        let mut local_names = HashSet::new();
        local_names.insert("Collection".to_string());

        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);

        // Collection stays qualified, others short
        assert!(resolved.qualified.contains(&CommonType::Collection));
        assert!(resolved.short.contains_key(&CommonType::Did));
        assert!(resolved.short.contains_key(&CommonType::RecordError));

        let use_block = resolved.to_use_block().to_string();
        // Both Did and RecordError in the block, but Collection not
        assert!(use_block.contains("Did"));
        assert!(use_block.contains("RecordError"));
        assert!(!use_block.contains("Collection,") && !use_block.contains("Collection}"));
    }

    #[test]
    fn test_resolved_imports_empty() {
        // Edge case: empty ImportSet
        let imports = ImportSet::default();
        let local_names = HashSet::new();

        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);

        assert!(resolved.short.is_empty());
        assert!(resolved.qualified.is_empty());
        assert!(resolved.external_short.is_empty());
        assert_eq!(resolved.to_use_block().to_string(), "");
    }

    #[test]
    fn test_resolved_imports_type_tokens() {
        let mut imports = ImportSet::default();
        imports.common.insert(CommonType::Did);

        let local_names = HashSet::new();
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);

        // For a short-named type with lifetime, type_tokens should emit ident<'a>.
        let tokens = resolved.type_tokens(&CommonType::Did);
        let tokens_str = tokens.to_string();
        assert_eq!(tokens_str, "Did < 'a >");

        // For a type without lifetime, type_tokens should emit just the ident.
        let mut imports2 = ImportSet::default();
        imports2.common.insert(CommonType::Datetime);
        let resolved2 = ResolvedImports::resolve(&imports2, &local_names, CodegenMode::Pretty);
        let tokens2 = resolved2.type_tokens(&CommonType::Datetime);
        assert_eq!(tokens2.to_string(), "Datetime");
    }

    #[test]
    fn test_resolved_imports_type_tokens_qualified() {
        let mut imports = ImportSet::default();
        imports.common.insert(CommonType::Did);

        let local_names = HashSet::new();
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Macro);

        // In Macro mode, type_tokens should emit the fully-qualified path with lifetime.
        let tokens = resolved.type_tokens(&CommonType::Did);
        let tokens_str = tokens.to_string();
        assert!(tokens_str.contains("jacquard_common"));
        assert!(tokens_str.contains("string"));
        assert!(tokens_str.contains("Did"));
        assert!(
            tokens_str.contains("'a"),
            "Did should include lifetime in Macro mode"
        );
    }

    #[test]
    fn test_option_is_none_path_macro_mode() {
        let imports = ImportSet::default();
        let local_names = HashSet::new();
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Macro);
        assert_eq!(resolved.option_is_none_path(), "core::option::Option::is_none");
    }

    #[test]
    fn test_option_is_none_path_pretty_no_collision() {
        let imports = ImportSet::default();
        let local_names = HashSet::new();
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);
        assert_eq!(resolved.option_is_none_path(), "Option::is_none");
    }

    #[test]
    fn test_option_is_none_path_pretty_with_option_collision() {
        // If a local type named "Option" exists, the path must be fully-qualified.
        let imports = ImportSet::default();
        let mut local_names = HashSet::new();
        local_names.insert("Option".to_string());
        let resolved = ResolvedImports::resolve(&imports, &local_names, CodegenMode::Pretty);
        assert_eq!(resolved.option_is_none_path(), "core::option::Option::is_none");
    }
}
