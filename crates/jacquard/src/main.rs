use clap::Parser;
use jacquard_api::com_atproto::repo::create_record::*;

#[derive(Parser, Debug)]
#[command(author = "Orual", version, about)]
/// Application configuration
struct Args {
    /// whether to be verbose
    #[arg(short = 'v')]
    verbose: bool,

    /// an optional name to greet
    #[arg()]
    name: Option<String>,
}

fn main() {
    let client = reqwest::Client::new();
}
