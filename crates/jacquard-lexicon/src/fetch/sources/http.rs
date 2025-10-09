use super::LexiconSource;
use crate::lexicon::LexiconDoc;
use jacquard_common::IntoStatic;
use miette::{Result, miette};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HttpSource {
    pub url: String,
}

#[derive(Deserialize)]
struct UfosRecord {
    record: serde_json::Value,
}

impl HttpSource {
    fn parse_lexicon_value(
        value: &serde_json::Value,
        failed_nsids: &mut std::collections::HashSet<String>,
    ) -> Option<LexiconDoc<'static>> {
        match serde_json::to_string(value) {
            Ok(json) => match serde_json::from_str::<LexiconDoc>(&json) {
                Ok(doc) => Some(doc.into_static()),
                Err(_e) => {
                    // Track failed NSID for summary
                    if let serde_json::Value::Object(obj) = value {
                        if let Some(serde_json::Value::String(id)) = obj.get("id") {
                            failed_nsids.insert(id.clone());
                        }
                    }
                    None
                }
            },
            Err(_e) => {
                // Track failed NSID for summary
                if let serde_json::Value::Object(obj) = value {
                    if let Some(serde_json::Value::String(id)) = obj.get("id") {
                        failed_nsids.insert(id.clone());
                    }
                }
                None
            }
        }
    }
}

impl LexiconSource for HttpSource {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        let resp = reqwest::get(&self.url)
            .await
            .map_err(|e| miette!("Failed to fetch from {}: {}", self.url, e))?;

        let records: Vec<UfosRecord> = resp
            .json()
            .await
            .map_err(|e| miette!("Failed to parse JSON from {}: {}", self.url, e))?;

        let mut lexicons = HashMap::new();
        let mut failed_nsids = std::collections::HashSet::new();
        let total_fetched = records.len();

        for ufos_record in records {
            if let Some(doc) = Self::parse_lexicon_value(&ufos_record.record, &mut failed_nsids) {
                let nsid = doc.id.to_string();
                lexicons.insert(nsid, doc);
            }
        }

        if !failed_nsids.is_empty() {
            eprintln!(
                "Warning: Failed to parse {} out of {} lexicons from HTTP source",
                failed_nsids.len(),
                total_fetched
            );
        }

        Ok(lexicons)
    }
}
