#!/usr/bin/env cargo
//! Example: Discover schemas across the workspace without link-time discovery
//!
//! Run with: cargo run --example workspace_discovery

use jacquard_lexgen::schema_discovery::WorkspaceDiscovery;

fn main() -> miette::Result<()> {
    println!("Workspace Schema Discovery Example\n");

    // Create workspace discovery
    let discovery = WorkspaceDiscovery::new().verbose(true);

    // Scan workspace
    let schemas = discovery.scan()?;

    println!("\n━━━ Results ━━━");
    println!("Discovered {} schema types:\n", schemas.len());

    // Group by crate
    use std::collections::HashMap;
    let mut by_crate: HashMap<String, Vec<_>> = HashMap::new();

    for schema in &schemas {
        let crate_name = schema
            .source_path
            .components()
            .find_map(|c| {
                let s = c.as_os_str().to_str()?;
                if s.starts_with("jacquard-") || s == "jacquard" {
                    Some(s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        by_crate.entry(crate_name).or_default().push(schema);
    }

    for (crate_name, crate_schemas) in by_crate {
        println!("📦 {} ({} schemas)", crate_name, crate_schemas.len());
        for schema in crate_schemas {
            println!("   • {} ({})", schema.nsid, schema.type_name);
        }
        println!();
    }

    Ok(())
}
