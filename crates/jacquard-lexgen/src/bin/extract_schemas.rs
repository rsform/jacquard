//! Extract AT Protocol lexicon schemas via workspace discovery
//!
//! This binary scans the workspace for types with `#[derive(LexiconSchema)]`
//! and generates lexicon JSON files. Unlike inventory-based extraction, this
//! discovers schemas across the entire workspace without requiring linking.

use clap::Parser;
use jacquard_lexgen::schema_discovery::WorkspaceDiscovery;
use miette::Result;

/// Extract lexicon schemas from workspace source files
#[derive(Parser, Debug)]
#[command(name = "extract-schemas")]
#[command(about = "Extract AT Protocol lexicon schemas from workspace")]
#[command(long_about = r#"
Scans workspace source files for types with #[derive(LexiconSchema)] and
generates lexicon JSON files. This discovers all schemas in the workspace
without requiring types to be linked into the binary.

For inventory-based extraction (link-time discovery), see the extract_inventory example.

See: https://docs.rs/jacquard-lexgen/latest/jacquard_lexgen/schema_discovery/
"#)]
struct Args {
    /// Output directory for generated schema files
    #[arg(short, long, default_value = "lexicons")]
    output: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let discovery = WorkspaceDiscovery::new()
        .verbose(args.verbose);

    discovery.generate_and_write(args.output)?;

    Ok(())
}
