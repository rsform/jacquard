use jacquard_lexgen::schema_extraction::{ExtractOptions, SchemaExtractor};
use tempfile::TempDir;

#[test]
fn test_extract_all_creates_output_dir() {
    let temp_dir = TempDir::new().unwrap();

    let options = ExtractOptions {
        output_dir: temp_dir.path().to_path_buf(),
        verbose: false,
        filter: None,
        validate: true,
        pretty: true,
    };

    let extractor = SchemaExtractor::new(options);

    // This will discover any schemas registered via inventory in the binary
    // In a minimal test environment, this might be 0
    let result = extractor.extract_all();

    // Should succeed even if no schemas found
    assert!(result.is_ok());

    // Directory should exist
    assert!(temp_dir.path().exists());
}

#[test]
fn test_extract_with_filter() {
    let temp_dir = TempDir::new().unwrap();

    let options = ExtractOptions {
        output_dir: temp_dir.path().to_path_buf(),
        verbose: false,
        filter: Some("com.example.nonexistent".into()),
        validate: true,
        pretty: true,
    };

    let extractor = SchemaExtractor::new(options);
    let result = extractor.extract_all();

    // Should succeed (just won't write any files)
    assert!(result.is_ok());
}

#[test]
fn test_extract_with_verbose() {
    let temp_dir = TempDir::new().unwrap();

    let options = ExtractOptions {
        output_dir: temp_dir.path().to_path_buf(),
        verbose: true,
        filter: None,
        validate: true,
        pretty: true,
    };

    let extractor = SchemaExtractor::new(options);
    let result = extractor.extract_all();

    assert!(result.is_ok());
}

#[test]
fn test_extract_compact_json() {
    let temp_dir = TempDir::new().unwrap();

    let options = ExtractOptions {
        output_dir: temp_dir.path().to_path_buf(),
        verbose: false,
        filter: None,
        validate: true,
        pretty: false, // Compact JSON
    };

    let extractor = SchemaExtractor::new(options);
    let result = extractor.extract_all();

    assert!(result.is_ok());
}
