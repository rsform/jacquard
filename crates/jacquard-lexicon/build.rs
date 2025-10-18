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
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    // Generate docs for lex-fetch
    generate_docs_for_binary(
        &out_dir,
        cli::LexFetchArgs::command(),
        "lex-fetch",
    )?;

    // Generate docs for jacquard-codegen
    generate_docs_for_binary(
        &out_dir,
        cli::CodegenArgs::command(),
        "jacquard-codegen",
    )?;

    println!(
        "cargo:warning=Generated man pages and completions to {:?}",
        out_dir
    );

    Ok(())
}

fn generate_docs_for_binary(
    out_dir: &PathBuf,
    mut cmd: clap::Command,
    bin_name: &str,
) -> Result<()> {
    // Generate man page
    let man_dir = out_dir.join("man");
    fs::create_dir_all(&man_dir)?;

    let man = Man::new(cmd.clone());
    let mut man_buffer = Vec::new();
    man.render(&mut man_buffer)?;
    fs::write(man_dir.join(format!("{}.1", bin_name)), man_buffer)?;

    // Generate shell completions
    let comp_dir = out_dir.join("completions");
    fs::create_dir_all(&comp_dir)?;

    generate_to(shells::Bash, &mut cmd, bin_name, &comp_dir)?;
    generate_to(shells::Fish, &mut cmd, bin_name, &comp_dir)?;
    generate_to(shells::Zsh, &mut cmd, bin_name, &comp_dir)?;

    Ok(())
}
