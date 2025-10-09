mod atproto;
mod git;
mod http;
mod jsonfile;
mod local;
mod slices;

pub use atproto::AtProtoSource;
pub use git::GitSource;
pub use http::HttpSource;
pub use jsonfile::JsonFileSource;
pub use local::LocalSource;
pub use slices::SlicesSource;

use crate::lexicon::LexiconDoc;
use async_trait::async_trait;
use miette::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub source_type: SourceType,
    pub explicit_priority: Option<i32>,
}

impl Source {
    /// Get effective priority based on type and explicit override
    pub fn priority(&self) -> i32 {
        if let Some(p) = self.explicit_priority {
            return p;
        }

        // Default priorities
        match &self.source_type {
            SourceType::Local(_) => 100,    // Highest - dev work
            SourceType::JsonFile(_) => 75,  // High - bundled exports
            SourceType::Slices(_) => 60,    // High-middle - slices network
            SourceType::AtProto(_) => 50,   // Middle - canonical published
            SourceType::Http(_) => 25,      // Lower middle - indexed samples
            SourceType::Git(_) => 0,        // Lowest - might be stale
        }
    }

    pub async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        self.source_type.fetch().await
    }
}

#[derive(Debug, Clone)]
pub enum SourceType {
    AtProto(AtProtoSource),
    Git(GitSource),
    Http(HttpSource),
    JsonFile(JsonFileSource),
    Local(LocalSource),
    Slices(SlicesSource),
}

#[async_trait]
pub trait LexiconSource {
    fn fetch(&self) -> impl Future<Output = Result<HashMap<String, LexiconDoc<'_>>>>;
}

impl LexiconSource for SourceType {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        match self {
            SourceType::AtProto(s) => s.fetch().await,
            SourceType::Git(s) => s.fetch().await,
            SourceType::Http(s) => s.fetch().await,
            SourceType::JsonFile(s) => s.fetch().await,
            SourceType::Local(s) => s.fetch().await,
            SourceType::Slices(s) => s.fetch().await,
        }
    }
}
