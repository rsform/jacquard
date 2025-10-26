//! # Schema Extraction
//!
//! Extract AT Protocol lexicon schemas from Rust types via `inventory` discovery.
//!
//! ## Usage Pattern
//!
//! This module provides schema extraction for types implementing `LexiconSchema`.
//! The extraction binary discovers schemas at **link time** via `inventory`, so you need
//! to create a binary in your workspace that links your schema types.
//!
//! ### Simple Usage
//!
//! ```rust,no_run
//! // bin/extract_schemas.rs
//! use jacquard_lexgen::schema_extraction;
//!
//! // Import your types so they get linked
//! use my_app::models::*;
//!
//! fn main() -> miette::Result<()> {
//!     schema_extraction::run(
//!         "lexicons",  // output directory
//!         true,        // verbose
//!     )
//! }
//! ```
//!
//! ### Advanced Usage
//!
//! ```rust,no_run
//! use jacquard_lexgen::schema_extraction::{ExtractOptions, SchemaExtractor};
//! use my_app::models::*;  // Your schema types
//!
//! fn main() -> miette::Result<()> {
//!     let options = ExtractOptions {
//!         output_dir: "lexicons".into(),
//!         verbose: true,
//!         filter: Some("app.bsky".into()),  // Only extract app.bsky.* schemas
//!         validate: true,
//!         pretty: true,
//!     };
//!
//!     SchemaExtractor::new(options).extract_all()
//! }
//! ```
//!
//! ### Integration with Build Tools
//!
//! **Just:**
//! ```justfile
//! # Generate lexicon schemas from Rust types
//! extract-schemas:
//!     cargo run --bin extract-schemas
//! ```
//!
//! **Cargo xtask:**
//! ```rust,ignore
//! // xtask/src/main.rs
//! match args {
//!     "codegen" => {
//!         run_command("cargo", &["run", "--bin", "extract-schemas"])?;
//!     }
//! }
//! ```
//!
//! **Pre-commit hook:**
//! ```bash
//! #!/bin/bash
//! # Regenerate schemas when Rust files change
//! if git diff --cached --name-only | grep -E '\.rs$'; then
//!     cargo run --bin extract-schemas
//!     git add lexicons/*.json
//! fi
//! ```

use jacquard_lexicon::lexicon::LexiconDoc;
use jacquard_lexicon::schema::LexiconSchemaRef;
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Options for schema extraction
pub struct ExtractOptions {
    /// Output directory for generated schema files
    pub output_dir: PathBuf,
    /// Enable verbose output
    pub verbose: bool,
    /// Filter by NSID prefix (e.g., "app.bsky")
    pub filter: Option<String>,
    /// Validate schemas before writing
    pub validate: bool,
    /// Pretty-print JSON output
    pub pretty: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("lexicons"),
            verbose: false,
            filter: None,
            validate: true,
            pretty: true,
        }
    }
}

/// Run schema extraction with simple defaults
///
/// Convenience function for the common case. For more control, use [`SchemaExtractor`].
///
/// # Arguments
///
/// * `output_dir` - Directory to write schema files (will be created if needed)
/// * `verbose` - Print progress information
///
/// # Example
///
/// ```rust,no_run
/// use jacquard_lexgen::schema_extraction;
/// use my_app::models::*;  // Your types with #[derive(LexiconSchema)]
///
/// fn main() -> miette::Result<()> {
///     schema_extraction::run("lexicons", true)
/// }
/// ```
pub fn run(output_dir: impl AsRef<Path>, verbose: bool) -> Result<()> {
    let options = ExtractOptions {
        output_dir: output_dir.as_ref().to_path_buf(),
        verbose,
        ..Default::default()
    };

    SchemaExtractor::new(options).extract_all()
}

pub struct SchemaExtractor {
    options: ExtractOptions,
}

impl SchemaExtractor {
    pub fn new(options: ExtractOptions) -> Self {
        Self { options }
    }

    /// Extract all schemas from inventory
    pub fn extract_all(&self) -> Result<()> {
        if self.options.verbose {
            println!("Discovering schemas via inventory...");
        }

        // Collect all schema refs from inventory
        let refs: Vec<&LexiconSchemaRef> = inventory::iter::<LexiconSchemaRef>().collect();

        if self.options.verbose {
            println!("Found {} schema types", refs.len());
        }

        // Group by base NSID
        let grouped = self.group_by_base_nsid(&refs)?;

        // Create output directory
        fs::create_dir_all(&self.options.output_dir).into_diagnostic()?;

        // Process each group
        let mut written = 0;
        for (base_nsid, group_refs) in grouped {
            // Apply filter if specified
            if let Some(filter) = &self.options.filter {
                if !base_nsid.starts_with(filter) {
                    continue;
                }
            }

            if self.options.verbose {
                println!("Processing {} ({} types)", base_nsid, group_refs.len());
            }

            self.write_lexicon(&base_nsid, &group_refs)?;
            written += 1;
        }

        println!(
            "✓ Wrote {} lexicon files to {}",
            written,
            self.options.output_dir.display()
        );

        Ok(())
    }

    /// Group refs by base NSID (strip fragment suffix)
    fn group_by_base_nsid<'a>(
        &self,
        refs: &[&'a LexiconSchemaRef],
    ) -> Result<BTreeMap<String, Vec<&'a LexiconSchemaRef>>> {
        let mut groups: BTreeMap<String, Vec<&'a LexiconSchemaRef>> = BTreeMap::new();

        for schema_ref in refs {
            let nsid = schema_ref.nsid;

            // Split on # to get base NSID
            let base_nsid = if let Some(pos) = nsid.find('#') {
                &nsid[..pos]
            } else {
                nsid
            };

            groups
                .entry(base_nsid.to_string())
                .or_default()
                .push(schema_ref);
        }

        Ok(groups)
    }

    /// Write a single lexicon file
    fn write_lexicon(&self, base_nsid: &str, refs: &[&LexiconSchemaRef]) -> Result<()> {
        // Generate all schemas in this group
        let mut all_defs = BTreeMap::new();
        let mut primary_doc: Option<LexiconDoc> = None;

        for schema_ref in refs {
            let doc = (schema_ref.provider)();

            // Determine if this is the primary def or a fragment
            if schema_ref.nsid.contains('#') {
                // Fragment - extract def name and add to defs
                let fragment_name = schema_ref.nsid.split('#').nth(1).unwrap();

                // Merge defs from fragment doc
                for (def_name, def) in doc.defs {
                    // Use fragment name if def is "main", otherwise use as-is
                    let final_name = if def_name == "main" {
                        fragment_name.to_string()
                    } else {
                        def_name.to_string()
                    };
                    all_defs.insert(final_name, def);
                }
            } else {
                // Primary type - use as base doc
                primary_doc = Some(doc);
            }
        }

        // Build final doc
        let mut final_doc = primary_doc.unwrap_or_else(|| {
            // No primary doc - create one
            use jacquard_lexicon::lexicon::Lexicon;
            LexiconDoc {
                lexicon: Lexicon::Lexicon1,
                id: base_nsid.into(),
                revision: None,
                description: None,
                defs: BTreeMap::new(),
            }
        });

        // Merge in all defs (convert String keys to SmolStr)
        for (k, v) in all_defs {
            final_doc.defs.insert(k.into(), v);
        }

        // Validate if requested
        if self.options.validate {
            self.validate_schema(&final_doc)?;
        }

        // Serialize to JSON with "main" def first
        let json = self.serialize_with_main_first(&final_doc)?;

        // Write to file
        let filename = base_nsid.replace('.', "_") + ".json";
        let path = self.options.output_dir.join(&filename);

        fs::write(&path, json).into_diagnostic()?;

        if self.options.verbose {
            println!("  Wrote {} ({} defs)", filename, final_doc.defs.len());
        }

        Ok(())
    }

    /// Validate a schema document
    fn validate_schema(&self, doc: &LexiconDoc) -> Result<()> {
        // Must have at least one def
        if doc.defs.is_empty() {
            return Err(miette::miette!("lexicon {} has no defs", doc.id));
        }

        // Warn if no "main" def and doesn't follow .defs convention
        if !doc.defs.contains_key("main") {
            let id_str = doc.id.as_ref();
            if !id_str.ends_with(".defs") {
                eprintln!(
                    "⚠️  Warning: lexicon {} has no 'main' def - consider naming it {}.defs",
                    id_str, id_str
                );
                if self.options.verbose {
                    eprintln!(
                        "   Lexicons without a primary type should use the .defs suffix (e.g., app.bsky.actor.defs)"
                    );
                }
            }
        }

        // Validate NSID format
        if !is_valid_nsid(&doc.id) {
            return Err(miette::miette!("invalid NSID format: {}", doc.id));
        }

        Ok(())
    }

    /// Watch mode - regenerate on file changes
    pub fn watch(&self) -> Result<()> {
        println!("Watch mode not yet implemented");
        println!("Run with --help to see available options");
        Ok(())
    }

    /// Serialize a lexicon doc with "main" def first
    fn serialize_with_main_first(&self, doc: &LexiconDoc) -> Result<String> {
        use serde_json::{json, Map, Value};

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

        // Serialize with or without pretty printing
        if self.options.pretty {
            serde_json::to_string_pretty(&Value::Object(obj)).into_diagnostic()
        } else {
            serde_json::to_string(&Value::Object(obj)).into_diagnostic()
        }
    }
}

/// Validate NSID format: domain.name.record
fn is_valid_nsid(nsid: &str) -> bool {
    let parts: Vec<&str> = nsid.split('.').collect();

    // Must have at least 3 parts
    if parts.len() < 3 {
        return false;
    }

    // Each part must be valid
    for part in parts {
        if part.is_empty() {
            return false;
        }

        // Must be alphanumeric, hyphens, or underscores
        if !part
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_nsid() {
        assert!(is_valid_nsid("com.example.test"));
        assert!(is_valid_nsid("app.bsky.feed.post"));
        assert!(is_valid_nsid("com.example.with_underscore"));
        assert!(is_valid_nsid("com.example.with-hyphen"));

        assert!(!is_valid_nsid("com.example")); // Too short
        assert!(!is_valid_nsid("com")); // Too short
        assert!(!is_valid_nsid("com.example.invalid!")); // Invalid char
        assert!(!is_valid_nsid("com..example")); // Empty segment
    }

    #[test]
    fn test_group_by_base_nsid() {
        let refs = vec![
            LexiconSchemaRef {
                nsid: "com.example.test",
                provider: || todo!(),
            },
            LexiconSchemaRef {
                nsid: "com.example.test#fragment",
                provider: || todo!(),
            },
            LexiconSchemaRef {
                nsid: "com.example.other",
                provider: || todo!(),
            },
        ];

        let ref_ptrs: Vec<&LexiconSchemaRef> = refs.iter().collect();

        let extractor = SchemaExtractor::new(ExtractOptions {
            output_dir: PathBuf::from("test"),
            verbose: false,
            filter: None,
            validate: false,
            pretty: true,
        });

        let grouped = extractor.group_by_base_nsid(&ref_ptrs).unwrap();

        assert_eq!(grouped.len(), 2);
        assert!(grouped.contains_key("com.example.test"));
        assert!(grouped.contains_key("com.example.other"));
        assert_eq!(grouped["com.example.test"].len(), 2);
        assert_eq!(grouped["com.example.other"].len(), 1);
    }
}
