use clap::Parser;
use jacquard_lexicon::cli::CodegenArgs;
use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;

fn main() -> miette::Result<()> {
    let args = CodegenArgs::parse();

    println!("Loading lexicons from {:?}...", args.input);
    let corpus = LexiconCorpus::load_from_dir(&args.input)?;

    println!("Loaded {} lexicon documents", corpus.iter().count());

    println!("Generating code...");
    let codegen = CodeGenerator::new(&corpus, "crate".to_string());
    codegen.write_to_disk(&args.output)?;

    println!("Generated code to {:?}", args.output);

    Ok(())
}
