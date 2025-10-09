use super::LexiconSource;
use crate::lexicon::LexiconDoc;
use jacquard_common::IntoStatic;
use miette::{Result, miette};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SlicesSource {
    pub slice: String,
}

#[derive(Serialize)]
struct GetRecordsRequest {
    slice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct GetRecordsResponse {
    records: Vec<Value>,
    #[serde(default)]
    cursor: Option<String>,
}

impl LexiconSource for SlicesSource {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        let client = reqwest::Client::new();
        let base_url = "https://api.slices.network/xrpc";
        let endpoint = format!("{}/com.atproto.lexicon.schema.getRecords", base_url);

        let mut lexicons = HashMap::new();
        let mut cursor: Option<String> = None;
        let mut total_fetched = 0;
        let mut failed_nsids = std::collections::HashSet::new();
        let mut page_count = 0;
        const MAX_PAGES: usize = 200; // Safety limit

        loop {
            page_count += 1;
            if page_count > MAX_PAGES {
                eprintln!(
                    "Warning: Hit max page limit ({}) for slices source",
                    MAX_PAGES
                );
                break;
            }
            let req_body = GetRecordsRequest {
                slice: self.slice.clone(),
                limit: Some(100),
                cursor: cursor.clone(),
            };

            let resp = client
                .post(&endpoint)
                .json(&req_body)
                .send()
                .await
                .map_err(|e| miette!("Failed to fetch from slices API: {}", e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(miette!("Slices API returned error {}: {}", status, body));
            }

            let response: GetRecordsResponse = resp
                .json()
                .await
                .map_err(|e| miette!("Failed to parse response: {}", e))?;

            total_fetched += response.records.len();

            for record_data in response.records.iter() {
                match Self::parse_lexicon_record(&record_data, &mut failed_nsids) {
                    Some(doc) => {
                        let nsid = doc.id.to_string();
                        lexicons.insert(nsid, doc);
                    }
                    None => {}
                }
            }

            let new_cursor = response.cursor;

            // Detect if we got no new results - API might be looping
            if response.records.is_empty() {
                break;
            }

            // Detect duplicate cursor
            if new_cursor == cursor {
                eprintln!("Warning: Slices API returned same cursor, stopping pagination");
                break;
            }

            cursor = new_cursor;
            if cursor.is_none() {
                break;
            }
        }

        if !failed_nsids.is_empty() {
            eprintln!(
                "Warning: Failed to parse {} out of {} lexicons from slices",
                failed_nsids.len(),
                total_fetched
            );
        }

        Ok(lexicons)
    }
}

impl SlicesSource {
    fn parse_lexicon_record(
        record_data: &Value,
        failed_nsids: &mut std::collections::HashSet<String>,
    ) -> Option<LexiconDoc<'static>> {
        // Extract the 'value' field from the record
        let value = record_data.get("value")?;

        // Convert to JSON string and then parse to handle lifetimes properly
        match serde_json::to_string(value) {
            Ok(json) => match serde_json::from_str::<LexiconDoc>(&json) {
                Ok(doc) => Some(doc.into_static()),
                Err(_e) => {
                    // Track failed NSID for summary
                    if let Value::Object(obj) = value {
                        if let Some(Value::String(id)) = obj.get("id") {
                            failed_nsids.insert(id.clone());
                        }
                    }
                    None
                }
            },
            Err(_e) => {
                // Track failed NSID for summary
                if let Value::Object(obj) = value {
                    if let Some(Value::String(id)) = obj.get("id") {
                        failed_nsids.insert(id.clone());
                    }
                }
                None
            }
        }
    }
}
