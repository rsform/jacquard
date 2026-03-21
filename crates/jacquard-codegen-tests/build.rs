use jacquard_lexicon::codegen::{CodeGenerator, CodegenMode};
use jacquard_lexicon::corpus::LexiconCorpus;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = manifest_dir.join("fixtures");

    let corpus = LexiconCorpus::load_from_dir(&fixtures_dir).expect("load test lexicons");

    // Generate Pretty mode output into src/generated/pretty/.
    // We generate into src/ (not OUT_DIR) because `mod` declarations
    // resolve relative to the source file, and include!() cannot handle
    // multi-file module trees.
    let pretty_dir = manifest_dir.join("src/generated/pretty");
    if pretty_dir.exists() {
        std::fs::remove_dir_all(&pretty_dir).expect("clean pretty output");
    }
    std::fs::create_dir_all(&pretty_dir).unwrap();
    let codegen = CodeGenerator::with_mode(&corpus, "crate::pretty", CodegenMode::Pretty);
    codegen
        .write_to_disk(&pretty_dir)
        .expect("pretty mode codegen");

    // Generate Macro mode output into src/generated/macro_mode/.
    let macro_dir = manifest_dir.join("src/generated/macro_mode");
    if macro_dir.exists() {
        std::fs::remove_dir_all(&macro_dir).expect("clean macro output");
    }
    std::fs::create_dir_all(&macro_dir).unwrap();
    let codegen = CodeGenerator::with_mode(&corpus, "crate::macro_mode", CodegenMode::Macro);
    codegen
        .write_to_disk(&macro_dir)
        .expect("macro mode codegen");

    println!("cargo::rerun-if-changed=fixtures");
}
