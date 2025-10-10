use super::LexiconSource;
use crate::{fetch::sources::parse_from_index_or_lexicon_file, lexicon::LexiconDoc};
use jacquard_common::IntoStatic;
use miette::{IntoDiagnostic, Result, miette};
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct GitSource {
    pub repo: String,
    pub git_ref: Option<String>,
    pub pattern: String,
}

impl LexiconSource for GitSource {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        // Create temp directory for clone
        let temp_dir = TempDir::new().into_diagnostic()?;
        let clone_path = temp_dir.path();

        // Shallow clone
        let mut clone_cmd = Command::new("git");
        clone_cmd.arg("clone").arg("--depth").arg("1");

        if let Some(ref git_ref) = self.git_ref {
            clone_cmd.arg("--branch").arg(git_ref);
        }

        clone_cmd.arg(&self.repo).arg(clone_path);

        let output = clone_cmd.output().await.into_diagnostic()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(miette!("Git clone failed: {}", stderr));
        }

        // Find lexicon files matching pattern
        let mut lexicons = HashMap::new();

        for entry in glob::glob(&format!("{}/{}", clone_path.display(), self.pattern))
            .into_diagnostic()?
            .filter_map(|e| e.ok())
        {
            if !entry.is_file() {
                continue;
            }

            // Try to parse as lexicon
            let content = tokio::fs::read_to_string(&entry).await.into_diagnostic()?;

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
