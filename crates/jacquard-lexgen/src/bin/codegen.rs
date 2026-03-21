use clap::Parser;
use jacquard_lexgen::cli::CodegenArgs;
use jacquard_lexicon::codegen::{CodeGenerator, CodegenMode};
use jacquard_lexicon::corpus::LexiconCorpus;

fn main() -> miette::Result<()> {
    let args = CodegenArgs::parse();
    let mode = if args.macro_mode {
        CodegenMode::Macro
    } else {
        CodegenMode::Pretty
    };

    println!("Loading lexicons from {:?}...", args.input);
    let corpus = LexiconCorpus::load_from_dir(&args.input)?;

    println!("Loaded {} lexicon documents", corpus.iter().count());

    println!("Generating code (mode: {:?})...", mode);
    let codegen = CodeGenerator::with_mode(&corpus, "crate".to_string(), mode);
    codegen.write_to_disk(&args.output)?;

    println!("Generated code to {:?}", args.output);

    Ok(())
}
