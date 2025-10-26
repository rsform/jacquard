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
//!     // Discover all schemas in workspace
//!     let schemas = WorkspaceDiscovery::new()
//!         .scan()?;
//!
//!     println!("Found {} schemas", schemas.len());
//!
//!     for schema in schemas {
//!         println!("  {}: {}", schema.nsid, schema.source_path.display());
//!     }
//!
//!     Ok(())
//! }
//! ```

use miette::{IntoDiagnostic, Result};
use std::path::{Path, PathBuf};
use syn::{Attribute, Item};

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
                }).into_diagnostic()?;
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
