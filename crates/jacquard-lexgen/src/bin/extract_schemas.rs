use clap::Parser;
use jacquard_lexgen::schema_extraction::{ExtractOptions, SchemaExtractor};
use miette::Result;

/// Extract lexicon schemas from compiled Rust types
#[derive(Parser, Debug)]
#[command(name = "extract-schemas")]
#[command(about = "Extract AT Protocol lexicon schemas from Rust types")]
struct Args {
    /// Output directory for generated schema files
    #[arg(short, long, default_value = "lexicons")]
    output: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Filter by NSID prefix (e.g., "app.bsky")
    #[arg(short, long)]
    filter: Option<String>,

    /// Validate schemas before writing
    #[arg(short = 'V', long, default_value = "true")]
    validate: bool,

    /// Pretty-print JSON output
    #[arg(short, long, default_value = "true")]
    pretty: bool,

    /// Watch mode - regenerate on changes
    #[arg(short, long)]
    watch: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let options = ExtractOptions {
        output_dir: args.output.into(),
        verbose: args.verbose,
        filter: args.filter,
        validate: args.validate,
        pretty: args.pretty,
    };

    let extractor = SchemaExtractor::new(options);

    if args.watch {
        extractor.watch()?;
    } else {
        extractor.extract_all()?;
    }

    Ok(())
}
