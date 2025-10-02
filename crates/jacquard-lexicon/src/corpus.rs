use crate::lexicon::{LexiconDoc, LexUserType};
use jacquard_common::{into_static::IntoStatic, smol_str::SmolStr};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// Registry of all loaded lexicons for reference resolution
#[derive(Debug, Clone)]
pub struct LexiconCorpus {
    /// Map from NSID to lexicon document
    docs: BTreeMap<SmolStr, LexiconDoc<'static>>,
}

impl LexiconCorpus {
    /// Create an empty corpus
    pub fn new() -> Self {
        Self {
            docs: BTreeMap::new(),
        }
    }

    /// Load all lexicons from a directory
    pub fn load_from_dir(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut corpus = Self::new();

        let schemas = crate::fs::find_schemas(path.as_ref())?;
        for schema_path in schemas {
            let content = fs::read_to_string(schema_path.as_ref())?;
            let doc: LexiconDoc = serde_json::from_str(&content).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed to parse {}: {}", schema_path.as_ref().display(), e),
                )
            })?;

            let nsid = SmolStr::from(doc.id.to_string());
            corpus.docs.insert(nsid, doc.into_static());
        }

        Ok(corpus)
    }

    /// Get a lexicon document by NSID
    pub fn get(&self, nsid: &str) -> Option<&LexiconDoc<'static>> {
        self.docs.get(nsid)
    }

    /// Resolve a reference, handling fragments
    ///
    /// Examples:
    /// - `app.bsky.feed.post` → main def from that lexicon
    /// - `app.bsky.feed.post#replyRef` → replyRef def from that lexicon
    pub fn resolve_ref(&self, ref_str: &str) -> Option<(&LexiconDoc<'static>, &LexUserType<'static>)> {
        let (nsid, def_name) = if let Some((nsid, fragment)) = ref_str.split_once('#') {
            (nsid, fragment)
        } else {
            (ref_str, "main")
        };

        let doc = self.get(nsid)?;
        let def = doc.defs.get(def_name)?;
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
    fn test_load_real_lexicons() {
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/lexicons")
            .expect("failed to load lexicons");

        assert!(!corpus.is_empty());
        assert_eq!(corpus.len(), 10);

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
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/lexicons")
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
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/lexicons")
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
        let corpus = LexiconCorpus::load_from_dir("tests/fixtures/lexicons")
            .expect("failed to load lexicons");

        // Existing refs
        assert!(corpus.ref_exists("app.bsky.feed.post"));
        assert!(corpus.ref_exists("app.bsky.feed.post#main"));
        assert!(corpus.ref_exists("app.bsky.richtext.facet#mention"));

        // Non-existing refs
        assert!(!corpus.ref_exists("com.example.fake"));
        assert!(!corpus.ref_exists("app.bsky.feed.post#nonexistent"));
    }
}
