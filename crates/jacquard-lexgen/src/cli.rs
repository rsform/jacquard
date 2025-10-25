use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Fetch Lexicon schemas from various sources")]
pub struct LexFetchArgs {
    /// Path to KDL config file
    #[arg(short = 'c', long, default_value = "lexicons.kdl")]
    pub config: PathBuf,

    /// Skip code generation step
    #[arg(long)]
    pub no_codegen: bool,

    /// Verbose output
    #[arg(short = 'v', long)]
    pub verbose: bool,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate Rust code from Lexicon schemas")]
pub struct CodegenArgs {
    /// Directory containing Lexicon JSON files
    #[arg(short = 'i', long)]
    pub input: PathBuf,

    /// Output directory for generated Rust code
    #[arg(short = 'o', long)]
    pub output: PathBuf,

    // TODO: root_module causes issues when set to anything other than "crate", needs rework
    // /// Root module name (default: "crate")
    // #[arg(short = 'r', long, default_value = "crate")]
    // pub root_module: String,
}
