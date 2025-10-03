use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;

#[test]
#[ignore] // Run with: cargo test --test regen_api -- --ignored
fn regenerate_api() {
    let corpus = LexiconCorpus::load_from_dir("tests/fixtures/lexicons/atproto/lexicons").expect("load corpus");
    let codegen = CodeGenerator::new(&corpus, "crate");

    codegen
        .write_to_disk(std::path::Path::new("../jacquard-api/src"))
        .expect("write to disk");

    println!("Generated {} lexicons", corpus.len());
}
