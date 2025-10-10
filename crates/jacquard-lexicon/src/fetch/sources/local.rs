use super::LexiconSource;
use crate::fetch::sources::parse_from_index_or_lexicon_file;
use crate::lexicon::LexiconDoc;
use jacquard_common::IntoStatic;
use miette::{IntoDiagnostic, Result};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalSource {
    pub path: PathBuf,
    pub pattern: Option<String>,
}

impl LexiconSource for LocalSource {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        let mut lexicons = HashMap::new();

        // Find all JSON files recursively
        for entry in walkdir::WalkDir::new(&self.path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // Try to parse as lexicon
            let content = std::fs::read_to_string(path).into_diagnostic()?;
            match parse_from_index_or_lexicon_file(&content) {
                Ok((nsid, doc)) => {
                    let doc = doc.into_static();
                    lexicons.insert(nsid, doc);
                }
                Err(_) => {
                    // Not a lexicon, skip
                    continue;
                }
            }
        }

        Ok(lexicons)
    }
}
