//! # Workspace Schema Discovery
//!
//! Alternative to inventory-based discovery that scans workspace source files directly.
//! This avoids the link-time limitation and can discover schemas across the entire workspace.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use jacquard_lexgen::schema_discovery::WorkspaceDiscovery;
//!
//! fn main() -> miette::Result<()> {
//!     // Discover and generate schemas
//!     WorkspaceDiscovery::new()
//!         .verbose(true)
//!         .generate_and_write("lexicons")?;
//!
//!     Ok(())
//! }
//! ```

use jacquard_lexicon::lexicon::LexiconDoc;
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::{Attribute, DeriveInput, Item};

/// Discovered schema type
#[derive(Debug, Clone)]
pub struct DiscoveredSchema {
    /// The NSID from the lexicon attribute
    pub nsid: String,
    /// Source file containing this type
    pub source_path: PathBuf,
    /// The type name
    pub type_name: String,
    /// Whether this is a struct or enum
    pub kind: SchemaKind,
    /// Parsed lexicon attributes
    pub attributes: LexiconAttributes,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaKind {
    Struct,
    Enum,
}

/// Parsed lexicon attributes from source
#[derive(Debug, Clone, Default)]
pub struct LexiconAttributes {
    pub nsid: Option<String>,
    pub fragment: Option<String>,
    pub record: bool,
    pub query: bool,
    pub procedure: bool,
    pub key: Option<String>,
}

/// Generated schema with full LexiconDoc
#[derive(Debug, Clone)]
pub struct GeneratedSchema {
    /// The NSID from the generated schema
    pub nsid: String,
    /// The schema_id (may include fragment)
    pub schema_id: String,
    /// The generated lexicon document
    pub doc: LexiconDoc<'static>,
    /// Source file containing this type
    pub source_path: PathBuf,
}

/// Workspace schema discovery via source scanning
pub struct WorkspaceDiscovery {
    workspace_root: PathBuf,
    verbose: bool,
}

impl WorkspaceDiscovery {
    /// Create a new workspace discovery
    ///
    /// Defaults to current directory as workspace root.
    pub fn new() -> Self {
        Self {
            workspace_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            verbose: false,
        }
    }

    /// Set the workspace root directory
    pub fn workspace_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace_root = path.into();
        self
    }

    /// Enable verbose output
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Scan workspace for schema types
    pub fn scan(&self) -> Result<Vec<DiscoveredSchema>> {
        if self.verbose {
            println!("Scanning workspace at {}", self.workspace_root.display());
        }

        let members = self.find_workspace_members()?;

        if self.verbose {
            println!("Found {} workspace members", members.len());
        }

        let mut schemas = Vec::new();

        for member in members {
            if self.verbose {
                println!("Scanning member: {}", member.display());
            }

            let member_schemas = self.scan_member(&member)?;
            schemas.extend(member_schemas);
        }

        if self.verbose {
            println!("Discovered {} total schemas", schemas.len());
        }

        Ok(schemas)
    }

    /// Scan workspace and generate complete schemas
    pub fn scan_and_generate(&self) -> Result<Vec<GeneratedSchema>> {
        let discovered = self.scan()?;

        if self.verbose {
            println!("Generating schemas for {} types...", discovered.len());
        }

        let mut generated = Vec::new();

        for schema_info in discovered {
            if self.verbose {
                println!(
                    "Generating schema for {}: {}",
                    schema_info.type_name, schema_info.nsid
                );
            }

            // Re-parse the source file to get full AST
            let contents = std::fs::read_to_string(&schema_info.source_path).into_diagnostic()?;
            let file = syn::parse_file(&contents).into_diagnostic()?;

            // Find the specific type
            let ast = self.find_type_in_file(&file, &schema_info.type_name)?;

            // Use schema builder based on kind
            let built = match schema_info.kind {
                SchemaKind::Struct => jacquard_lexicon::schema::from_ast::build_struct_schema(&ast)
                    .into_diagnostic()?,
                SchemaKind::Enum => {
                    jacquard_lexicon::schema::from_ast::build_enum_schema(&ast).into_diagnostic()?
                }
            };

            generated.push(GeneratedSchema {
                nsid: built.nsid,
                schema_id: built.schema_id,
                doc: built.doc,
                source_path: schema_info.source_path.clone(),
            });
        }

        if self.verbose {
            println!("Generated {} schemas", generated.len());
        }

        Ok(generated)
    }

    /// Generate schemas and write to directory
    pub fn generate_and_write(&self, output_dir: impl AsRef<Path>) -> Result<()> {
        let schemas = self.scan_and_generate()?;

        if schemas.is_empty() {
            println!("No schemas found to generate");
            return Ok(());
        }

        // Group by base NSID (strip #fragments)
        let grouped = self.group_by_base_nsid(&schemas);

        // Create output directory
        std::fs::create_dir_all(output_dir.as_ref()).into_diagnostic()?;

        // Write each group
        let mut written = 0;
        for (base_nsid, group) in &grouped {
            self.write_lexicon_file(output_dir.as_ref(), base_nsid, group)?;
            written += 1;
        }

        println!(
            "✓ Wrote {} lexicon files to {}",
            written,
            output_dir.as_ref().display()
        );

        Ok(())
    }

    /// Group schemas by base NSID (strip fragment suffix)
    fn group_by_base_nsid<'a>(
        &self,
        schemas: &'a [GeneratedSchema],
    ) -> BTreeMap<String, Vec<&'a GeneratedSchema>> {
        let mut groups: BTreeMap<String, Vec<&'a GeneratedSchema>> = BTreeMap::new();

        for schema in schemas {
            // Split on # to get base NSID
            let base_nsid = if let Some(pos) = schema.nsid.find('#') {
                &schema.nsid[..pos]
            } else {
                &schema.nsid
            };

            groups
                .entry(base_nsid.to_string())
                .or_default()
                .push(schema);
        }

        groups
    }

    /// Write a single lexicon file
    fn write_lexicon_file(
        &self,
        output_dir: &Path,
        base_nsid: &str,
        schemas: &[&GeneratedSchema],
    ) -> Result<()> {
        use jacquard_lexicon::lexicon::Lexicon;

        // Merge all defs into one LexiconDoc
        let mut all_defs = BTreeMap::new();
        let mut primary_doc: Option<LexiconDoc> = None;

        for schema in schemas {
            // Determine if this is the primary def or a fragment
            if schema.nsid.contains('#') {
                // Fragment - extract def name and add to defs
                let fragment_name = schema.nsid.split('#').nth(1).unwrap();

                // Merge defs from fragment doc
                for (def_name, def) in &schema.doc.defs {
                    // Use fragment name if def is "main", otherwise use as-is
                    let final_name = if def_name == "main" {
                        fragment_name.to_string()
                    } else {
                        def_name.to_string()
                    };
                    all_defs.insert(final_name, def.clone());
                }
            } else {
                // Primary type - use as base doc
                primary_doc = Some(schema.doc.clone());
            }
        }

        // Build final doc
        let mut final_doc = primary_doc.unwrap_or_else(|| LexiconDoc {
            lexicon: Lexicon::Lexicon1,
            id: base_nsid.into(),
            revision: None,
            description: None,
            defs: BTreeMap::new(),
        });

        // Merge in all defs
        for (k, v) in all_defs {
            final_doc.defs.insert(k.into(), v);
        }

        // Serialize to JSON with "main" def first
        let json = self.serialize_with_main_first(&final_doc)?;

        // Write to file
        let filename = base_nsid.replace('.', "_") + ".json";
        let path = output_dir.join(&filename);

        std::fs::write(&path, json).into_diagnostic()?;

        if self.verbose {
            println!("  Wrote {} ({} defs)", filename, final_doc.defs.len());
        }

        Ok(())
    }

    /// Serialize a lexicon doc with "main" def first
    fn serialize_with_main_first(&self, doc: &LexiconDoc) -> Result<String> {
        use serde_json::{Map, Value, json};

        // Build defs map with main first
        let mut defs_map = Map::new();

        // Insert main first if it exists
        if let Some(main_def) = doc.defs.get("main") {
            let main_value = serde_json::to_value(main_def).into_diagnostic()?;
            defs_map.insert("main".to_string(), main_value);
        }

        // Insert all other defs in sorted order
        for (name, def) in &doc.defs {
            if name != "main" {
                let def_value = serde_json::to_value(def).into_diagnostic()?;
                defs_map.insert(name.to_string(), def_value);
            }
        }

        // Build final JSON object
        let mut obj = Map::new();
        obj.insert("lexicon".to_string(), json!(1));
        obj.insert("id".to_string(), json!(doc.id.as_ref()));

        if let Some(rev) = &doc.revision {
            obj.insert("revision".to_string(), json!(rev));
        }

        if let Some(desc) = &doc.description {
            obj.insert("description".to_string(), json!(desc));
        }

        obj.insert("defs".to_string(), Value::Object(defs_map));

        // Pretty-print JSON
        serde_json::to_string_pretty(&Value::Object(obj)).into_diagnostic()
    }

    /// Find workspace members by parsing Cargo.toml
    fn find_workspace_members(&self) -> Result<Vec<PathBuf>> {
        let cargo_toml = self.workspace_root.join("Cargo.toml");

        if !cargo_toml.exists() {
            return Err(miette::miette!(
                "Cargo.toml not found at {}",
                cargo_toml.display()
            ));
        }

        let contents = std::fs::read_to_string(&cargo_toml).into_diagnostic()?;
        let manifest: toml::Value = toml::from_str(&contents).into_diagnostic()?;

        let mut members = Vec::new();

        // Check if this is a workspace
        if let Some(workspace) = manifest.get("workspace") {
            if let Some(member_list) = workspace.get("members").and_then(|v| v.as_array()) {
                for member in member_list {
                    if let Some(pattern) = member.as_str() {
                        // Handle glob patterns like "crates/*"
                        let glob_pattern = self.workspace_root.join(pattern).join("Cargo.toml");

                        for entry in glob::glob(glob_pattern.to_str().unwrap()).into_diagnostic()? {
                            let path = entry.into_diagnostic()?;
                            if let Some(dir) = path.parent() {
                                members.push(dir.to_path_buf());
                            }
                        }
                    }
                }
            }
        } else {
            // Not a workspace - just this crate
            members.push(self.workspace_root.clone());
        }

        Ok(members)
    }

    /// Scan a single workspace member
    fn scan_member(&self, member_path: &Path) -> Result<Vec<DiscoveredSchema>> {
        let src_dir = member_path.join("src");

        if !src_dir.exists() {
            return Ok(Vec::new());
        }

        let mut schemas = Vec::new();

        // Recursively scan all .rs files
        for entry in walkdir::WalkDir::new(&src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(file_schemas) = self.scan_file(entry.path()) {
                    schemas.extend(file_schemas);
                }
            }
        }

        Ok(schemas)
    }

    /// Scan a single Rust source file
    fn scan_file(&self, path: &Path) -> Result<Vec<DiscoveredSchema>> {
        let contents = std::fs::read_to_string(path).into_diagnostic()?;
        let file = syn::parse_file(&contents).into_diagnostic()?;

        let mut schemas = Vec::new();

        for item in file.items {
            match item {
                Item::Struct(item_struct) => {
                    if let Some(schema) = self.extract_schema_from_attrs(
                        &item_struct.attrs,
                        &item_struct.ident.to_string(),
                        SchemaKind::Struct,
                        path,
                    )? {
                        schemas.push(schema);
                    }
                }
                Item::Enum(item_enum) => {
                    if let Some(schema) = self.extract_schema_from_attrs(
                        &item_enum.attrs,
                        &item_enum.ident.to_string(),
                        SchemaKind::Enum,
                        path,
                    )? {
                        schemas.push(schema);
                    }
                }
                _ => {}
            }
        }

        Ok(schemas)
    }

    /// Find a type in a parsed file and convert to DeriveInput
    fn find_type_in_file(&self, file: &syn::File, type_name: &str) -> Result<DeriveInput> {
        for item in &file.items {
            match item {
                Item::Struct(item_struct) if item_struct.ident == type_name => {
                    // Convert ItemStruct to DeriveInput
                    return Ok(DeriveInput {
                        attrs: item_struct.attrs.clone(),
                        vis: item_struct.vis.clone(),
                        ident: item_struct.ident.clone(),
                        generics: item_struct.generics.clone(),
                        data: syn::Data::Struct(syn::DataStruct {
                            struct_token: item_struct.struct_token,
                            fields: item_struct.fields.clone(),
                            semi_token: item_struct.semi_token,
                        }),
                    });
                }
                Item::Enum(item_enum) if item_enum.ident == type_name => {
                    // Convert ItemEnum to DeriveInput
                    return Ok(DeriveInput {
                        attrs: item_enum.attrs.clone(),
                        vis: item_enum.vis.clone(),
                        ident: item_enum.ident.clone(),
                        generics: item_enum.generics.clone(),
                        data: syn::Data::Enum(syn::DataEnum {
                            enum_token: item_enum.enum_token,
                            brace_token: item_enum.brace_token,
                            variants: item_enum.variants.clone(),
                        }),
                    });
                }
                _ => continue,
            }
        }

        Err(miette::miette!(
            "Type {} not found in source file",
            type_name
        ))
    }

    /// Extract schema info from attributes
    fn extract_schema_from_attrs(
        &self,
        attrs: &[Attribute],
        type_name: &str,
        kind: SchemaKind,
        source_path: &Path,
    ) -> Result<Option<DiscoveredSchema>> {
        // Check if this type has #[derive(LexiconSchema)]
        let has_derive = attrs.iter().any(|attr| {
            if attr.path().is_ident("derive") {
                if let Ok(meta) = attr.parse_args::<syn::Meta>() {
                    if let syn::Meta::Path(path) = meta {
                        return path.is_ident("LexiconSchema");
                    }
                }
            }
            false
        });

        if !has_derive {
            return Ok(None);
        }

        // Parse #[lexicon(...)] attributes
        let mut lex_attrs = LexiconAttributes::default();

        for attr in attrs {
            if attr.path().is_ident("lexicon") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("nsid") {
                        let value = meta.value()?;
                        let lit: syn::LitStr = value.parse()?;
                        lex_attrs.nsid = Some(lit.value());
                    } else if meta.path.is_ident("fragment") {
                        if meta.input.peek(syn::Token![=]) {
                            let value = meta.value()?;
                            let lit: syn::LitStr = value.parse()?;
                            lex_attrs.fragment = Some(lit.value());
                        } else {
                            lex_attrs.fragment = Some(String::new());
                        }
                    } else if meta.path.is_ident("record") {
                        lex_attrs.record = true;
                    } else if meta.path.is_ident("query") {
                        lex_attrs.query = true;
                    } else if meta.path.is_ident("procedure") {
                        lex_attrs.procedure = true;
                    } else if meta.path.is_ident("key") {
                        let value = meta.value()?;
                        let lit: syn::LitStr = value.parse()?;
                        lex_attrs.key = Some(lit.value());
                    }
                    Ok(())
                })
                .into_diagnostic()?;
            }
        }

        let nsid = lex_attrs.nsid.clone().ok_or_else(|| {
            miette::miette!(
                "Type {} has #[derive(LexiconSchema)] but no nsid attribute",
                type_name
            )
        })?;

        Ok(Some(DiscoveredSchema {
            nsid,
            source_path: source_path.to_path_buf(),
            type_name: type_name.to_string(),
            kind,
            attributes: lex_attrs,
        }))
    }
}

impl Default for WorkspaceDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_struct() {
        let code = r#"
            #[derive(LexiconSchema)]
            #[lexicon(nsid = "com.example.test", record)]
            struct Test {
                field: String,
            }
        "#;

        let file = syn::parse_file(code).unwrap();
        let discovery = WorkspaceDiscovery::new();

        if let Item::Struct(item_struct) = &file.items[0] {
            let schema = discovery
                .extract_schema_from_attrs(
                    &item_struct.attrs,
                    "Test",
                    SchemaKind::Struct,
                    Path::new("test.rs"),
                )
                .unwrap()
                .unwrap();

            assert_eq!(schema.nsid, "com.example.test");
            assert_eq!(schema.type_name, "Test");
            assert!(schema.attributes.record);
        } else {
            panic!("Expected struct");
        }
    }
}
