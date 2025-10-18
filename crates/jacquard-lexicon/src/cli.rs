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
