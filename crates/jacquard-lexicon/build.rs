use clap::CommandFactory;
use clap_complete::{generate_to, shells};
use clap_mangen::Man;
use std::env;
use std::fs;
use std::io::Result;
use std::path::PathBuf;

#[path = "src/cli.rs"]
mod cli;

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| ".".to_string()));
    let mut cmd = cli::LexFetchArgs::command();

    // Generate man page
    let man_dir = out_dir.join("man");
    fs::create_dir_all(&man_dir)?;

    let man = Man::new(cmd.clone());
    let mut man_buffer = Vec::new();
    man.render(&mut man_buffer)?;
    fs::write(man_dir.join("lex-fetch.1"), man_buffer)?;

    // Generate shell completions
    let comp_dir = out_dir.join("completions");
    fs::create_dir_all(&comp_dir)?;

    generate_to(shells::Bash, &mut cmd, "lex-fetch", &comp_dir)?;
    generate_to(shells::Fish, &mut cmd, "lex-fetch", &comp_dir)?;
    generate_to(shells::Zsh, &mut cmd, "lex-fetch", &comp_dir)?;

    println!(
        "cargo:warning=Generated man page and completions to {:?}",
        out_dir
    );

    Ok(())
}
