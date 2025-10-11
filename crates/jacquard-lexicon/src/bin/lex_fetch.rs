use clap::Parser;
use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;
use jacquard_lexicon::fetch::{Config, Fetcher};
use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Fetch Lexicon schemas from various sources")]
struct Args {
    /// Path to KDL config file
    #[arg(short = 'c', long, default_value = "lexicons.kdl")]
    config: PathBuf,

    /// Skip code generation step
    #[arg(long)]
    no_codegen: bool,

    /// Verbose output
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.verbose {
        println!("Reading config from {:?}...", args.config);
    }

    let config_text = std::fs::read_to_string(&args.config).into_diagnostic()?;

    // Parse KDL config
    let config = Config::from_kdl(&config_text)?;

    // Fetch from all sources
    if args.verbose {
        println!("Fetching lexicons from {} sources...", config.sources.len());
    }

    let fetcher = Fetcher::new(config.clone());
    let lexicons = fetcher.fetch_all(args.verbose).await?;

    if args.verbose || !args.no_codegen {
        println!("Fetched {} unique lexicons", lexicons.len());
    }

    // Ensure output directory exists
    std::fs::create_dir_all(&config.output.lexicons_dir).into_diagnostic()?;

    // Write each lexicon to a file
    for (nsid, doc) in &lexicons {
        let filename = format!("{}.json", nsid.replace('.', "_"));
        let path = config.output.lexicons_dir.join(&filename);

        let json = serde_json::to_string_pretty(doc).into_diagnostic()?;
        std::fs::write(&path, json).into_diagnostic()?;

        if args.verbose {
            println!("Wrote {}", filename);
        }
    }

    // Run codegen if requested
    if !args.no_codegen {
        if args.verbose {
            println!("Generating code...");
        }

        let corpus = LexiconCorpus::load_from_dir(&config.output.lexicons_dir)?;
        let root_module = config
            .output
            .root_module
            .unwrap_or_else(|| "crate".to_string());
        let codegen = CodeGenerator::new(&corpus, root_module);
        std::fs::create_dir_all(&config.output.codegen_dir).into_diagnostic()?;
        codegen.write_to_disk(&config.output.codegen_dir)?;

        println!("Generated code to {:?}", config.output.codegen_dir);

        // Update Cargo.toml features if cargo_toml_path is specified
        if let Some(cargo_toml_path) = &config.output.cargo_toml_path {
            if args.verbose {
                println!("Updating Cargo.toml features...");
            }

            update_cargo_features(&codegen, cargo_toml_path, &config.output.codegen_dir)?;
            println!("Updated features in {:?}", cargo_toml_path);
        }
    } else {
        println!("Lexicons written to {:?}", config.output.lexicons_dir);
    }

    Ok(())
}

fn update_cargo_features(codegen: &CodeGenerator, cargo_toml_path: &PathBuf, codegen_dir: &PathBuf) -> Result<()> {
    // Read existing Cargo.toml
    let content = std::fs::read_to_string(cargo_toml_path).into_diagnostic()?;

    // Find the "# --- generated ---" marker
    const MARKER: &str = "# --- generated ---";

    let (before, _after) = content.split_once(MARKER)
        .ok_or_else(|| miette::miette!("Cargo.toml missing '{}' marker", MARKER))?;

    // Generate new features, passing lib.rs path to detect existing modules
    let lib_rs_path = codegen_dir.join("lib.rs");
    let features = codegen.generate_cargo_features(Some(&lib_rs_path));

    // Reconstruct file
    let new_content = format!("{}{}\n{}", before, MARKER, features);

    // Write back
    std::fs::write(cargo_toml_path, new_content).into_diagnostic()?;

    Ok(())
}
