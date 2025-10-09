use super::LexiconSource;
use crate::lexicon::LexiconDoc;
use jacquard_api::com_atproto::repo::list_records::ListRecords;
use jacquard_common::IntoStatic;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::Nsid;
use jacquard_common::xrpc::XrpcExt;
use jacquard_identity::JacquardResolver;
use jacquard_identity::resolver::{IdentityResolver, ResolverOptions};
use miette::{Result, miette};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AtProtoSource {
    pub endpoint: String,
    pub slice: Option<String>,
}

impl AtProtoSource {
    fn parse_lexicon_record(
        record_data: &jacquard_common::types::value::Data<'_>,
    ) -> Option<LexiconDoc<'static>> {
        // Extract the 'value' field from the record
        let value = match record_data {
            jacquard_common::types::value::Data::Object(map) => map.0.get("value")?,
            _ => {
                eprintln!("Warning: Record is not an object");
                return None;
            }
        };

        match serde_json::to_string(value) {
            Ok(json) => match serde_json::from_str::<LexiconDoc>(&json) {
                Ok(doc) => Some(doc.into_static()),
                Err(e) => {
                    eprintln!("Warning: Failed to parse lexicon from record value: {}", e);
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

impl LexiconSource for AtProtoSource {
    async fn fetch(&self) -> Result<HashMap<String, LexiconDoc<'_>>> {
        let http = reqwest::Client::new();
        let resolver = JacquardResolver::new(http, ResolverOptions::default());

        // Parse endpoint as at-identifier (handle or DID)
        let identifier = AtIdentifier::new(&self.endpoint)
            .map_err(|e| miette!("Invalid endpoint '{}': {}", self.endpoint, e))?;

        // Resolve to get PDS endpoint
        let did = match &identifier {
            AtIdentifier::Did(d) => d.clone().into_static(),
            AtIdentifier::Handle(h) => resolver.resolve_handle(h).await?,
        };

        let did_doc_resp = resolver.resolve_did_doc(&did).await?;

        let did_doc = did_doc_resp.parse()?;

        let pds = did_doc
            .pds_endpoint()
            .ok_or_else(|| miette!("No PDS endpoint found for {}", did))?;

        // Determine repo - use slice if provided, otherwise use the resolved DID
        let repo = if let Some(ref slice) = self.slice {
            AtIdentifier::new(slice)
                .map_err(|e| miette!("Invalid slice '{}': {}", slice, e))?
                .into_static()
        } else {
            AtIdentifier::Did(did.clone())
        };

        let collection = Nsid::new("com.atproto.lexicon.schema")
            .map_err(|e| miette!("Invalid collection NSID: {}", e))?;

        let mut lexicons = HashMap::new();

        // Try to fetch all records at once first
        let req = ListRecords::new()
            .repo(repo.clone().into_static())
            .collection(collection.clone().into_static())
            .build();

        let resp = resolver.xrpc(pds.clone()).send(&req).await?;

        match resp.into_output() {
            Ok(output) => {
                // Batch fetch succeeded
                for record_data in output.records {
                    if let Some(doc) = Self::parse_lexicon_record(&record_data) {
                        let nsid = doc.id.to_string();
                        lexicons.insert(nsid, doc);
                    }
                }
            }
            Err(e) => {
                // Batch decode failed, try one-by-one with cursor
                eprintln!("Warning: Batch decode failed from {}: {}", self.endpoint, e);
                eprintln!("Retrying with limit=1 to skip invalid records...");

                let mut cursor: Option<String> = None;
                loop {
                    let req = if let Some(ref c) = cursor {
                        ListRecords::new()
                            .repo(repo.clone().into_static())
                            .collection(collection.clone().into_static())
                            .limit(1)
                            .cursor(c.clone())
                            .build()
                    } else {
                        ListRecords::new()
                            .repo(repo.clone().into_static())
                            .collection(collection.clone().into_static())
                            .limit(1)
                            .build()
                    };
                    let resp = resolver.xrpc(pds.clone()).send(&req).await?;

                    match resp.into_output() {
                        Ok(output) => {
                            for record_data in output.records {
                                if let Some(doc) = Self::parse_lexicon_record(&record_data) {
                                    let nsid = doc.id.to_string();
                                    lexicons.insert(nsid, doc);
                                }
                            }

                            if let Some(next_cursor) = output.cursor {
                                cursor = Some(next_cursor.to_string());
                            } else {
                                break;
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to decode record (skipping): {}", e);
                            // Try to continue with next record if possible
                            // This is a bit tricky since we don't have the cursor from failed decode
                            // For now, just break
                            break;
                        }
                    }
                }
            }
        }

        Ok(lexicons)
    }
}
