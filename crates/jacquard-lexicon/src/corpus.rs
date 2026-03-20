use crate::error::{CodegenError, Result};
use crate::lexicon::{LexUserType, LexiconDoc};
use crate::ref_utils::RefPath;
use jacquard_common::{deps::smol_str::SmolStr, into_static::IntoStatic};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Check if content looks like a lexicon file.
///
/// A file is considered a lexicon if it contains a `"lexicon"` key at the top level
/// or one level down (for some wrapper formats). This allows us to distinguish
/// "not a lexicon at all" (skip silently) from "broken lexicon" (report error).
fn is_lexicon_content(content: &str) -> bool {
    // Quick string scan first (fast path for non-JSON or unrelated JSON)
    if !content.contains("\"lexicon\"") {
        return false;
    }

    // Parse to Value and check structure
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        // Top-level lexicon field
        if value.get("lexicon").is_some() {
            return true;
        }
        // One level down (some wrapper formats)
        if let Some(obj) = value.as_object() {
            for v in obj.values() {
                if v.get("lexicon").is_some() {
                    return true;
                }
            }
        }
    }
    false
}

/// Raw lexicon doc for two-phase parsing - defs are kept as raw JSON Values
/// so we can deserialize each separately with better error tracking.
#[derive(Debug, serde::Deserialize)]
struct RawLexiconDoc<'s> {
    pub lexicon: crate::lexicon::Lexicon,
    #[serde(borrow)]
    pub id: jacquard_common::CowStr<'s>,
    pub revision: Option<u32>,
    #[serde(borrow)]
    pub description: Option<jacquard_common::CowStr<'s>>,
    pub defs: BTreeMap<SmolStr, serde_json::Value>,
}

/// Helper to create a parse error with path context.
fn make_parse_error(
    file_path: &Path,
    json_path: &str,
    message: String,
    content: &str,
) -> CodegenError {
    CodegenError::ParseError {
        path: file_path.to_path_buf(),
        json_path: Some(json_path.to_string()),
        message,
        src: Some(content.to_string()),
        span: None,
    }
}

/// Recursively parse properties with path tracking.
/// Returns parsed properties or an error with the full path.
fn parse_properties_deep(
    props_value: &serde_json::Value,
    base_path: &str,
    file_path: &Path,
    content: &str,
) -> std::result::Result<BTreeMap<SmolStr, crate::lexicon::LexObjectProperty<'static>>, CodegenError>
{
    let props_obj = props_value.as_object().ok_or_else(|| {
        make_parse_error(
            file_path,
            base_path,
            "expected object for properties".to_string(),
            content,
        )
    })?;

    let mut parsed_props = BTreeMap::new();
    for (prop_name, prop_value) in props_obj {
        let prop_path = format!("{}.{}", base_path, prop_name);

        // Try to parse this property
        let parsed: crate::lexicon::LexObjectProperty =
            serde_path_to_error::deserialize(prop_value).map_err(|e| {
                let inner_path = e.path().to_string();
                let full_path = if inner_path.is_empty() {
                    prop_path.clone()
                } else {
                    format!("{}.{}", prop_path, inner_path)
                };
                make_parse_error(file_path, &full_path, e.inner().to_string(), content)
            })?;

        parsed_props.insert(SmolStr::new(prop_name), parsed.into_static());
    }

    Ok(parsed_props)
}

/// Parse an object-like def with deep property tracking.
fn parse_object_deep(
    value: &serde_json::Value,
    base_path: &str,
    file_path: &Path,
    content: &str,
) -> std::result::Result<crate::lexicon::LexObject<'static>, CodegenError> {
    use crate::lexicon::LexObject;

    let obj = value.as_object().ok_or_else(|| {
        make_parse_error(file_path, base_path, "expected object".to_string(), content)
    })?;

    // Parse properties deeply if present
    let properties = if let Some(props) = obj.get("properties") {
        let props_path = format!("{}.properties", base_path);
        parse_properties_deep(props, &props_path, file_path, content)?
    } else {
        BTreeMap::new()
    };

    // Parse the rest of the object normally
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| jacquard_common::CowStr::copy_from_str(s));
    let required: Option<Vec<SmolStr>> = obj
        .get("required")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| {
            make_parse_error(
                file_path,
                &format!("{}.required", base_path),
                e.to_string(),
                content,
            )
        })?;
    let nullable: Option<Vec<SmolStr>> = obj
        .get("nullable")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| {
            make_parse_error(
                file_path,
                &format!("{}.nullable", base_path),
                e.to_string(),
                content,
            )
        })?;

    Ok(LexObject {
        description,
        required,
        nullable,
        properties,
    })
}

/// Parse a def with deep path tracking for nested structures.
fn parse_def_deep(
    def_name: &str,
    value: &serde_json::Value,
    file_path: &Path,
    content: &str,
) -> std::result::Result<LexUserType<'static>, CodegenError> {
    let base_path = format!("defs.{}", def_name);

    // Check the type field to determine how to parse
    let type_str = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("object");

    match type_str {
        "object" => {
            let obj = parse_object_deep(value, &base_path, file_path, content)?;
            Ok(LexUserType::Object(obj))
        }
        "record" => {
            // Records have a nested record.properties structure
            if let Some(record_value) = value.get("record") {
                let record_path = format!("{}.record", base_path);
                let inner_obj = parse_object_deep(record_value, &record_path, file_path, content)?;

                // Parse the rest of the record
                let obj = value.as_object().ok_or_else(|| {
                    make_parse_error(
                        file_path,
                        &base_path,
                        "expected object".to_string(),
                        content,
                    )
                })?;

                let description = obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| jacquard_common::CowStr::copy_from_str(s));
                let key: Option<jacquard_common::CowStr<'static>> = obj
                    .get("key")
                    .and_then(|v| v.as_str())
                    .map(|s| jacquard_common::CowStr::copy_from_str(s));

                Ok(LexUserType::Record(crate::lexicon::LexRecord {
                    description,
                    key,
                    record: crate::lexicon::LexRecordRecord::Object(inner_obj),
                }))
            } else {
                // Fallback to normal parsing if no record field
                serde_path_to_error::deserialize(value)
                    .map(|v: LexUserType| v.into_static())
                    .map_err(|e| {
                        make_parse_error(file_path, &base_path, e.inner().to_string(), content)
                    })
            }
        }
        // For other types (query, procedure, etc.), use the simpler approach for now
        // Could be extended later
        _ => serde_path_to_error::deserialize(value)
            .map(|v: LexUserType| v.into_static())
            .map_err(|e| {
                let inner_path = e.path().to_string();
                let full_path = if inner_path.is_empty() {
                    base_path
                } else {
                    format!("{}.{}", base_path, inner_path)
                };
                make_parse_error(file_path, &full_path, e.inner().to_string(), content)
            }),
    }
}

/// Parse a lexicon with rich error context using deep recursive parsing.
///
/// This parses the document structure recursively, tracking paths through:
/// - defs → def_name → properties → prop_name → nested fields
///
/// This gives us detailed error paths like "defs.main.properties.count.default"
fn parse_lexicon_with_context(
    content: &str,
    path: &Path,
) -> std::result::Result<LexiconDoc<'static>, CodegenError> {
    // Phase 1: Parse the top-level structure with defs as raw Values
    let raw_doc: RawLexiconDoc =
        serde_json::from_str(content).map_err(|e| CodegenError::ParseError {
            path: path.to_path_buf(),
            json_path: None,
            message: e.to_string(),
            src: Some(content.to_string()),
            span: None,
        })?;

    // Phase 2: Parse each def with deep path tracking
    let mut parsed_defs = BTreeMap::new();
    for (def_name, def_value) in raw_doc.defs {
        let parsed_def = parse_def_deep(&def_name, &def_value, path, content)?;
        parsed_defs.insert(def_name, parsed_def);
    }

    // Reconstruct the full LexiconDoc
    Ok(LexiconDoc {
        lexicon: raw_doc.lexicon,
        id: raw_doc.id.into_static(),
        revision: raw_doc.revision,
        description: raw_doc.description.map(|d| d.into_static()),
        defs: parsed_defs,
    })
}

/// Registry of all loaded lexicons for reference resolution
#[derive(Debug, Clone)]
pub struct LexiconCorpus {
    /// Map from NSID to lexicon document
    docs: BTreeMap<SmolStr, LexiconDoc<'static>>,
    /// Map from NSID to original source text (for error reporting)
    sources: BTreeMap<SmolStr, String>,
}

impl LexiconCorpus {
    /// Create an empty corpus
    pub fn new() -> Self {
        Self {
            docs: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }

    /// Load all lexicons from a directory
    pub fn load_from_dir(path: impl AsRef<Path>) -> Result<Self> {
        let mut corpus = Self::new();

        let schemas = crate::fs::find_schemas(path.as_ref())?;
        for schema_path in schemas {
            let content = fs::read_to_string(schema_path.as_ref())?;

            // Check if this file is trying to be a lexicon
            if !is_lexicon_content(&content) {
                // Not a lexicon, skip silently
                continue;
            }

            // This IS a lexicon - parse with good error reporting
            let doc = parse_lexicon_with_context(&content, schema_path.as_ref())?;

            let nsid = SmolStr::from(doc.id.to_string());
            corpus.docs.insert(nsid.clone(), doc);
            corpus.sources.insert(nsid, content);
        }

        Ok(corpus)
    }

    /// Get a lexicon document by NSID
    pub fn get(&self, nsid: &str) -> Option<&LexiconDoc<'static>> {
        self.docs.get(nsid)
    }

    /// Get the source text for a lexicon by NSID
    pub fn get_source(&self, nsid: &str) -> Option<&str> {
        self.sources.get(nsid).map(|s| s.as_str())
    }

    /// Resolve a reference, handling fragments
    ///
    /// Examples:
    /// - `app.bsky.feed.post` → main def from that lexicon
    /// - `app.bsky.feed.post#replyRef` → replyRef def from that lexicon
    pub fn resolve_ref(
        &self,
        ref_str: &str,
    ) -> Option<(&LexiconDoc<'static>, &LexUserType<'static>)> {
        let ref_path = RefPath::parse(ref_str, None);
        let doc = self.get(ref_path.nsid())?;
        let def = doc.defs.get(ref_path.def())?;
        Some((doc, def))
    }

    /// Check if a reference exists
    pub fn ref_exists(&self, ref_str: &str) -> bool {
        self.resolve_ref(ref_str).is_some()
    }

    /// Iterate over all documents
    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &LexiconDoc<'static>)> {
        self.docs.iter()
    }

    /// Number of loaded lexicons
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Check if corpus is empty
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }
}

impl Default for LexiconCorpus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexicon::LexUserType;

    #[test]
    fn test_empty_corpus() {
        let corpus = LexiconCorpus::new();
        assert!(corpus.is_empty());
        assert_eq!(corpus.len(), 0);
    }

    #[test]
    fn test_load_lexicons() {
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons")
            .expect("failed to load lexicons");

        assert!(!corpus.is_empty());
        assert_eq!(corpus.len(), 17); // 10 original + 7 new edge case fixtures

        // Check that we loaded the expected lexicons
        assert!(corpus.get("app.bsky.feed.post").is_some());
        assert!(corpus.get("app.bsky.feed.getAuthorFeed").is_some());
        assert!(corpus.get("app.bsky.richtext.facet").is_some());
        assert!(corpus.get("app.bsky.embed.images").is_some());
        assert!(corpus.get("com.atproto.repo.strongRef").is_some());
        assert!(corpus.get("com.atproto.label.defs").is_some());
    }

    #[test]
    fn test_resolve_ref_without_fragment() {
        let corpus = LexiconCorpus::load_from_dir("../jacquard-api/lexicons")
            .expect("failed to load lexicons");

        // Without fragment should resolve to main def
        let (doc, def) = corpus
            .resolve_ref("app.bsky.feed.post")
            .expect("should resolve");
        assert_eq!(doc.id.as_ref(), "app.bsky.feed.post");
        assert!(matches!(def, LexUserType::Record(_)));
    }

    #[test]
    fn test_resolve_ref_with_fragment() {
        let corpus = LexiconCorpus::load_from_dir("../jacquard-api/lexicons")
            .expect("failed to load lexicons");

        // With fragment should resolve to specific def
        let (doc, def) = corpus
            .resolve_ref("app.bsky.richtext.facet#mention")
            .expect("should resolve");
        assert_eq!(doc.id.as_ref(), "app.bsky.richtext.facet");
        assert!(matches!(def, LexUserType::Object(_)));
    }

    #[test]
    fn test_ref_exists() {
        let corpus = LexiconCorpus::load_from_dir("../jacquard-api/lexicons")
            .expect("failed to load lexicons");

        // Existing refs
        assert!(corpus.ref_exists("app.bsky.feed.post"));
        assert!(corpus.ref_exists("app.bsky.feed.post#main"));
        assert!(corpus.ref_exists("app.bsky.richtext.facet#mention"));

        // Non-existing refs
        assert!(!corpus.ref_exists("com.example.fake"));
        assert!(!corpus.ref_exists("app.bsky.feed.post#nonexistent"));
    }

    #[test]
    fn test_non_lexicon_json_skipped_silently() {
        // The test_lexicons directory contains not_a_lexicon.json which should be skipped
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/test_lexicons")
            .expect("should succeed even with non-lexicon JSON files");

        // The non-lexicon file should not be in the corpus
        assert!(corpus.get("some random config").is_none());

        // But valid lexicons should still load
        assert!(corpus.get("app.bsky.feed.post").is_some());
    }

    #[test]
    fn test_is_lexicon_content_detection() {
        // Not a lexicon - no "lexicon" key
        assert!(!is_lexicon_content(r#"{"name": "test", "version": "1.0"}"#));

        // Not a lexicon - invalid JSON
        assert!(!is_lexicon_content("not json at all"));

        // Is a lexicon - has "lexicon" at top level
        assert!(is_lexicon_content(r#"{"lexicon": 1, "id": "test.foo"}"#));

        // Is a lexicon - has "lexicon" one level down
        assert!(is_lexicon_content(
            r#"{"wrapper": {"lexicon": 1, "id": "test.foo"}}"#
        ));
    }

    #[test]
    fn test_broken_lexicon_returns_error_with_path() {
        let result = LexiconCorpus::load_from_dir("tests/fixtures/error_cases");

        // Should fail because broken_lexicon.json is a lexicon (has "lexicon" key)
        // but has invalid structure
        let err = result.expect_err("should fail on broken lexicon");
        let err_str = err.to_string();

        // Error should include the full path to the broken property
        assert!(
            err_str.contains("defs.main.properties.count"),
            "error should contain path to the broken property, got: {}",
            err_str
        );

        // Error should also include the actual error message
        assert!(
            err_str.contains("expected i64"),
            "error should describe the type mismatch, got: {}",
            err_str
        );

        // Error should mention the file
        assert!(
            err_str.contains("broken_lexicon.json"),
            "error should mention the file, got: {}",
            err_str
        );
    }
}
