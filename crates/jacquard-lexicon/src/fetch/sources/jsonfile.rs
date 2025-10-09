use super::LexiconSource;
use crate::lexicon::LexiconDoc;
use jacquard_common::types::value::Data;
use jacquard_common::IntoStatic;
use miette::{IntoDiagnostic, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct JsonFileSource {
    pub path: PathBuf,
}

#[derive(Deserialize)]
struct RecordsFile<'a> {
    #[serde(borrow)]
    records: Vec<Data<'a>>,
}

impl LexiconSource for JsonFileSource {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        let content = std::fs::read_to_string(&self.path).into_diagnostic()?;
        let file: RecordsFile = serde_json::from_str(&content).into_diagnostic()?;

        let mut lexicons = HashMap::new();

        for record_data in file.records {
            if let Some(doc) = Self::parse_lexicon_record(&record_data) {
                let nsid = doc.id.to_string();
                lexicons.insert(nsid, doc);
            }
        }

        Ok(lexicons)
    }
}

impl JsonFileSource {
    fn parse_lexicon_record(record_data: &Data<'_>) -> Option<LexiconDoc<'static>> {
        let value = match record_data {
            Data::Object(map) => map.0.get("value")?,
            _ => return None,
        };

        match serde_json::to_string(value) {
            Ok(json) => match serde_json::from_str::<LexiconDoc>(&json) {
                Ok(doc) => Some(doc.into_static()),
                Err(e) => {
                    eprintln!("Warning: Failed to parse lexicon from record: {}", e);
                    None
                }
            },
            Err(e) => {
                eprintln!("Warning: Failed to serialize record value: {}", e);
                None
            }
        }
    }
}
