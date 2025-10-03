use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;
use prettyplease;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lexicons_path = "lexicons/atproto";
    let output_path = "crates/jacquard-api/src";
    let root_module = "crate";

    println!("Loading lexicons from {}...", lexicons_path);
    let corpus = LexiconCorpus::load_from_dir(lexicons_path)?;
    println!("Loaded {} lexicons", corpus.len());

    println!("Generating code...");
    let generator = CodeGenerator::new(&corpus, root_module);

    // Group by module
    let mut modules: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for (nsid, doc) in corpus.iter() {
        let nsid_str = nsid.as_str();

        // Get module path: app.bsky.feed.post -> app_bsky/feed
        let parts: Vec<&str> = nsid_str.split('.').collect();
        let module_path = if parts.len() >= 3 {
            let first_two = format!("{}_{}", parts[0], parts[1]);
            if parts.len() > 3 {
                let middle: Vec<&str> = parts[2..parts.len() - 1].iter().copied().collect();
                format!("{}/{}", first_two, middle.join("/"))
            } else {
                first_two
            }
        } else {
            parts.join("_")
        };

        let file_name = parts.last().unwrap().to_string();

        for (def_name, def) in &doc.defs {
            match generator.generate_def(nsid_str, def_name, def) {
                Ok(tokens) => {
                    let code = prettyplease::unparse(&syn::parse_file(&tokens.to_string())?);
                    modules
                        .entry(format!("{}/{}.rs", module_path, file_name))
                        .or_default()
                        .push((def_name.to_string(), code));
                }
                Err(e) => {
                    eprintln!("Error generating {}.{}: {:?}", nsid_str, def_name, e);
                }
            }
        }
    }

    // Write files
    for (file_path, defs) in modules {
        let full_path = Path::new(output_path).join(&file_path);

        // Create parent directory
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = defs.iter().map(|(_, code)| code.as_str()).collect::<Vec<_>>().join("\n");
        fs::write(&full_path, content)?;
        println!("Wrote {}", file_path);
    }

    // Generate mod.rs files
    println!("Generating mod.rs files...");
    generate_mod_files(Path::new(output_path))?;

    println!("Done!");
    Ok(())
}

fn generate_mod_files(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Find all directories
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_str().unwrap();

            // Recursively generate for subdirectories
            generate_mod_files(&path)?;

            // Generate mod.rs for this directory
            let mut mods = Vec::new();
            for sub_entry in fs::read_dir(&path)? {
                let sub_entry = sub_entry?;
                let sub_path = sub_entry.path();

                if sub_path.is_file() {
                    if let Some(name) = sub_path.file_stem() {
                        let name_str = name.to_str().unwrap();
                        if name_str != "mod" {
                            mods.push(format!("pub mod {};", name_str));
                        }
                    }
                } else if sub_path.is_dir() {
                    if let Some(name) = sub_path.file_name() {
                        mods.push(format!("pub mod {};", name.to_str().unwrap()));
                    }
                }
            }

            if !mods.is_empty() {
                let mod_content = mods.join("\n") + "\n";
                fs::write(path.join("mod.rs"), mod_content)?;
            }
        }
    }

    Ok(())
}
