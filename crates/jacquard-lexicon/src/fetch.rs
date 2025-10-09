pub mod config;
pub mod sources;

pub use config::Config;
use jacquard_common::IntoStatic;
pub use sources::{LexiconSource, SourceType};

use crate::lexicon::LexiconDoc;
use miette::Result;
use std::collections::HashMap;

/// Orchestrates fetching lexicons from multiple sources
pub struct Fetcher {
    config: Config,
}

impl Fetcher {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Fetch lexicons from all configured sources
    pub async fn fetch_all(&self, verbose: bool) -> Result<HashMap<String, LexiconDoc<'_>>> {
        let mut lexicons = HashMap::new();

        // Sort sources by priority (lowest first, so highest priority overwrites)
        let mut sources = self.config.sources.clone();
        sources.sort_by_key(|s| s.priority());

        for source in sources.iter() {
            if verbose {
                println!(
                    "Fetching from {} ({:?})...",
                    source.name, source.source_type
                );
            }

            let fetched = source.fetch().await?;

            if verbose {
                println!("  Found {} lexicons", fetched.len());
            }

            // Merge, with later sources overwriting earlier ones
            for (nsid, doc) in fetched {
                if let Some(_) = lexicons.get(&nsid) {
                    if verbose {
                        println!("  Overwriting {} (priority {})", nsid, source.priority());
                    }
                }
                lexicons.insert(nsid, doc);
            }
        }

        Ok(lexicons.into_static())
    }
}
