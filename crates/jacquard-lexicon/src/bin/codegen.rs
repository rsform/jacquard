use clap::Parser;
use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate Rust code from Lexicon schemas")]
struct Args {
    /// Directory containing Lexicon JSON files
    #[arg(short = 'i', long)]
    input: PathBuf,

    /// Output directory for generated Rust code
    #[arg(short = 'o', long)]
    output: PathBuf,

    /// Root module name (default: "crate")
    #[arg(short = 'r', long, default_value = "crate")]
    root_module: String,
}

fn main() -> miette::Result<()> {
    let args = Args::parse();

    println!("Loading lexicons from {:?}...", args.input);
    let corpus = LexiconCorpus::load_from_dir(&args.input)?;

    println!("Loaded {} lexicon documents", corpus.iter().count());

    println!("Generating code...");
    let codegen = CodeGenerator::new(&corpus, args.root_module);
    codegen.write_to_disk(&args.output)?;

    println!("✨ Generated code to {:?}", args.output);

    Ok(())
}
