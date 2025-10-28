//! Example demonstrating lexicon schema resolution via DNS + XRPC
//!
//! Run with: cargo run --example resolve_lexicon --features dns

use jacquard_common::types::string::Nsid;
use jacquard_identity::{
    JacquardResolver,
    lexicon_resolver::{LexiconAuthorityResolver, LexiconSchemaResolver},
    resolver::ResolverOptions,
};

#[tokio::main]
async fn main() -> miette::Result<()> {
    // Set up resolver with DNS enabled
    let http = reqwest::Client::new();
    let opts = ResolverOptions::default();
    let resolver = JacquardResolver::new_dns(http, opts);

    // Test NSID - using app.bsky.feed.post as a known lexicon
    let nsid =
        Nsid::new("app.bsky.feed.post").map_err(|e| miette::miette!("invalid NSID: {}", e))?;

    println!("Resolving lexicon for: {}", nsid);
    println!();
    println!("Resolving authority via DNS...");
    match resolver.resolve_lexicon_authority(&nsid).await {
        Ok(did) => {
            println!("Authority DID: {}", did);
        }
        Err(e) => {
            eprintln!("Failed to resolve authority: {:?}", e);
            return Err(e.into());
        }
    }

    println!();
    println!("Fetching full lexicon schema...");
    match resolver.resolve_lexicon_schema(&nsid).await {
        Ok(schema) => {
            println!("Successfully fetched schema");
            println!("  NSID: {}", schema.nsid);
            println!("  Repo: {}", schema.repo);
            println!("  CID: {}", schema.cid);
            println!("  Doc ID: {}", schema.doc.id);
            println!(
                "\nSchema:\n{}",
                serde_json::to_string_pretty(&schema.doc).unwrap()
            )
        }
        Err(e) => {
            eprintln!("Failed to resolve schema: {:?}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
