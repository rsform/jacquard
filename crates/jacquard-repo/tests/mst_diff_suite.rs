//! MST diff test suite runner
//!
//! Runs the mst-test-suite exhaustive diff test cases to validate:
//! - record_ops (creates/updates/deletes with CIDs)
//! - created_nodes (new MST blocks)
//! - deleted_nodes (removed MST blocks)
//! - proof_nodes (blocks needed for inclusion/exclusion proofs)
//! - inductive_proof_nodes (blocks needed for inductive validation)

use bytes::Bytes;
use cid::Cid as IpldCid;
use jacquard_repo::car::parse_car_bytes;
use jacquard_repo::mst::{Mst, MstDiff};
use jacquard_repo::storage::MemoryBlockStore;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TEST_SUITE_PATH: &str = "/home/orual/Git_Repos/mst-test-suite";

/// Test case format from mst-test-suite
#[derive(Debug, Deserialize, Serialize)]
struct MstDiffTestCase {
    #[serde(rename = "$type")]
    test_type: String,

    description: String,

    inputs: TestInputs,

    results: ExpectedResults,
}

#[derive(Debug, Deserialize, Serialize)]
struct TestInputs {
    /// Path to CAR file for tree A (relative to test suite root)
    mst_a: String,

    /// Path to CAR file for tree B (relative to test suite root)
    mst_b: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ExpectedResults {
    /// CIDs of newly created MST node blocks
    created_nodes: Vec<String>,

    /// CIDs of deleted MST node blocks
    deleted_nodes: Vec<String>,

    /// Record operations (sorted by rpath)
    record_ops: Vec<RecordOp>,

    /// CIDs of MST nodes required for inclusion/exclusion proofs
    proof_nodes: Vec<String>,

    /// CIDs of MST nodes required for inductive validation
    inductive_proof_nodes: Vec<String>,

    /// CIDs expected in firehose broadcast (mostly marked TODO in fixtures)
    #[serde(skip_serializing_if = "Option::is_none")]
    firehose_cids: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct RecordOp {
    /// Record path (rpath)
    rpath: String,

    /// Old CID (null for creates)
    old_value: Option<String>,

    /// New CID (null for deletes)
    new_value: Option<String>,
}

/// Load and parse a CAR file, returning blocks and root CID
async fn load_car(path: &Path) -> anyhow::Result<(IpldCid, BTreeMap<IpldCid, Bytes>)> {
    let bytes = tokio::fs::read(path).await?;
    let parsed = parse_car_bytes(&bytes).await?;
    Ok((parsed.root, parsed.blocks))
}

/// Convert base32 CID string to IpldCid
fn parse_cid(cid_str: &str) -> anyhow::Result<IpldCid> {
    Ok(cid_str.parse()?)
}

/// Convert IpldCid to base32 string (for comparison)
fn cid_to_string(cid: &IpldCid) -> String {
    cid.to_string()
}

/// Find all .json test files in a directory recursively
fn find_test_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut test_files = Vec::new();

    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                test_files.extend(find_test_files(&path)?);
            } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                test_files.push(path);
            }
        }
    }

    Ok(test_files)
}

/// Run a single test case
async fn run_test_case(test_path: &Path, suite_root: &Path) -> anyhow::Result<TestResult> {
    // Parse test case JSON
    let test_json = tokio::fs::read_to_string(test_path).await?;
    let test_case: MstDiffTestCase = serde_json::from_str(&test_json)?;

    // Load CAR files
    let car_a_path = suite_root.join(&test_case.inputs.mst_a);
    let car_b_path = suite_root.join(&test_case.inputs.mst_b);

    let (root_a, blocks_a) = load_car(&car_a_path).await?;
    let (root_b, blocks_b) = load_car(&car_b_path).await?;

    // Create storage with both sets of blocks
    let mut all_blocks = blocks_a;
    all_blocks.extend(blocks_b);
    let storage = Arc::new(MemoryBlockStore::new_from_blocks(all_blocks));

    // Load MST instances
    let mst_a = Mst::load(storage.clone(), root_a, None);
    let mst_b = Mst::load(storage.clone(), root_b, None);

    // Compute diff
    let diff = mst_a.diff(&mst_b).await?;

    // Replicate create_commit's relevant_blocks logic (from repo.rs:276-290)
    let mut relevant_blocks = BTreeMap::new();
    let ops_count = diff.creates.len() + diff.updates.len() + diff.deletes.len();

    // For each operation, collect blocks along the path in BOTH trees for inductive validation
    for (key, _cid) in &diff.creates {
        mst_b
            .blocks_for_path(key.as_str(), &mut relevant_blocks)
            .await?;
        // Always include old tree paths for CREATE (needed for exclusion proof)
        mst_a
            .blocks_for_path(key.as_str(), &mut relevant_blocks)
            .await?;
    }

    for (key, _new_cid, _old_cid) in &diff.updates {
        mst_b
            .blocks_for_path(key.as_str(), &mut relevant_blocks)
            .await?;
        // Include old tree paths for UPDATE (needed for inductive validation)
        mst_a
            .blocks_for_path(key.as_str(), &mut relevant_blocks)
            .await?;
    }

    for (key, _old_cid) in &diff.deletes {
        mst_b
            .blocks_for_path(key.as_str(), &mut relevant_blocks)
            .await?;
        // Include old tree paths for DELETE (needed for inductive validation)
        mst_a
            .blocks_for_path(key.as_str(), &mut relevant_blocks)
            .await?;
    }

    // Union of new_mst_blocks and relevant_blocks (for inductive proof)
    // NOTE: relevant_blocks may contain blocks from both old and new trees,
    // but we should exclude blocks that were deleted (in removed_mst_blocks)
    let removed_set: std::collections::HashSet<_> =
        diff.removed_mst_blocks.iter().copied().collect();
    let filtered_relevant: BTreeMap<_, _> = relevant_blocks
        .into_iter()
        .filter(|(cid, _)| !removed_set.contains(cid))
        .collect();

    let mut all_proof_blocks = diff.new_mst_blocks.clone();
    all_proof_blocks.extend(filtered_relevant);

    // Validate results
    let mut result = TestResult {
        test_name: test_path.file_name().unwrap().to_string_lossy().to_string(),
        description: test_case.description.clone(),
        passed: true,
        record_ops_match: false,
        created_nodes_match: false,
        deleted_nodes_match: false,
        proof_nodes_info: None,
        inductive_proof_nodes_info: None,
        errors: Vec::new(),
    };

    // Validate record_ops
    let actual_ops = diff_to_record_ops(&diff);
    let expected_ops = test_case.results.record_ops;
    result.record_ops_match = actual_ops == expected_ops;
    if !result.record_ops_match {
        result.errors.push(format!(
            "Record ops mismatch: expected {} ops, got {}",
            expected_ops.len(),
            actual_ops.len()
        ));
        result.passed = false;
    }

    // Validate created_nodes
    let actual_created: BTreeSet<String> = diff.new_mst_blocks.keys().map(cid_to_string).collect();
    let expected_created: BTreeSet<String> =
        test_case.results.created_nodes.iter().cloned().collect();
    result.created_nodes_match = actual_created == expected_created;
    if !result.created_nodes_match {
        result.errors.push(format!(
            "Created nodes mismatch: expected {}, got {}",
            expected_created.len(),
            actual_created.len()
        ));
        result.passed = false;
    }

    // Validate deleted_nodes
    let actual_deleted: BTreeSet<String> =
        diff.removed_mst_blocks.iter().map(cid_to_string).collect();
    let expected_deleted: BTreeSet<String> =
        test_case.results.deleted_nodes.iter().cloned().collect();
    result.deleted_nodes_match = actual_deleted == expected_deleted;
    if !result.deleted_nodes_match {
        result.errors.push(format!(
            "Deleted nodes mismatch: expected {}, got {}",
            expected_deleted.len(),
            actual_deleted.len()
        ));
        result.passed = false;
    }

    // Compare proof_nodes (should equal new_mst_blocks)
    let expected_proof: BTreeSet<String> = test_case.results.proof_nodes.iter().cloned().collect();
    let actual_proof: BTreeSet<String> = diff.new_mst_blocks.keys().map(cid_to_string).collect();
    let proof_match_status = compute_match_status(&actual_proof, &expected_proof);

    result.proof_nodes_info = Some(ProofNodesInfo {
        expected: expected_proof.clone(),
        actual: actual_proof.clone(),
        match_status: proof_match_status,
    });

    // Compare inductive_proof_nodes (should equal all_proof_blocks)
    let expected_inductive: BTreeSet<String> = test_case
        .results
        .inductive_proof_nodes
        .iter()
        .cloned()
        .collect();
    let actual_inductive: BTreeSet<String> = all_proof_blocks.keys().map(cid_to_string).collect();
    let inductive_match_status = compute_match_status(&actual_inductive, &expected_inductive);

    result.inductive_proof_nodes_info = Some(ProofNodesInfo {
        expected: expected_inductive.clone(),
        actual: actual_inductive.clone(),
        match_status: inductive_match_status,
    });

    Ok(result)
}

/// Compute match status between actual and expected sets
fn compute_match_status(actual: &BTreeSet<String>, expected: &BTreeSet<String>) -> MatchStatus {
    if actual == expected {
        MatchStatus::Exact
    } else if actual.is_subset(expected) {
        MatchStatus::Subset
    } else if actual.is_superset(expected) {
        MatchStatus::Superset
    } else {
        MatchStatus::Different
    }
}

/// Convert MstDiff to sorted record operations
fn diff_to_record_ops(diff: &MstDiff) -> Vec<RecordOp> {
    let mut ops = Vec::new();

    // Creates
    for (key, cid) in &diff.creates {
        ops.push(RecordOp {
            rpath: key.to_string(),
            old_value: None,
            new_value: Some(cid_to_string(cid)),
        });
    }

    // Updates
    for (key, new_cid, old_cid) in &diff.updates {
        ops.push(RecordOp {
            rpath: key.to_string(),
            old_value: Some(cid_to_string(old_cid)),
            new_value: Some(cid_to_string(new_cid)),
        });
    }

    // Deletes
    for (key, old_cid) in &diff.deletes {
        ops.push(RecordOp {
            rpath: key.to_string(),
            old_value: Some(cid_to_string(old_cid)),
            new_value: None,
        });
    }

    // Sort by rpath
    ops.sort();
    ops
}

/// Test result for a single test case
#[derive(Debug)]
struct TestResult {
    test_name: String,
    description: String,
    passed: bool,
    record_ops_match: bool,
    created_nodes_match: bool,
    deleted_nodes_match: bool,
    proof_nodes_info: Option<ProofNodesInfo>,
    inductive_proof_nodes_info: Option<ProofNodesInfo>,
    errors: Vec<String>,
}

#[derive(Debug)]
struct ProofNodesInfo {
    expected: BTreeSet<String>,
    actual: BTreeSet<String>,
    match_status: MatchStatus,
}

#[derive(Debug)]
enum MatchStatus {
    Exact,
    Subset,    // actual is subset of expected (missing blocks)
    Superset,  // actual is superset of expected (extra blocks)
    Different, // neither subset nor superset
    NotImplemented,
}

/// Summary statistics across all tests
#[derive(Debug, Default)]
struct TestSummary {
    total_tests: usize,
    passed_tests: usize,
    failed_tests: usize,
    record_ops_matches: usize,
    created_nodes_matches: usize,
    deleted_nodes_matches: usize,
    proof_exact_matches: usize,
    proof_subset_matches: usize,
    proof_superset_matches: usize,
    inductive_exact_matches: usize,
    inductive_subset_matches: usize,
    inductive_superset_matches: usize,
}

#[tokio::test]
#[ignore] // Local-only: requires mst-test-suite at /home/orual/Git_Repos/mst-test-suite
async fn run_mst_diff_suite() {
    let suite_root = Path::new(TEST_SUITE_PATH);
    let tests_dir = suite_root.join("tests");

    // Find all test files
    let test_files = find_test_files(&tests_dir).expect("Failed to find test files");

    println!("Found {} test files", test_files.len());

    let mut summary = TestSummary::default();
    let mut failed_tests = Vec::new();

    for test_path in &test_files {
        summary.total_tests += 1;

        match run_test_case(test_path, suite_root).await {
            Ok(result) => {
                let passed = result.passed;
                let record_ops_match = result.record_ops_match;
                let created_nodes_match = result.created_nodes_match;
                let deleted_nodes_match = result.deleted_nodes_match;

                // Track proof node match status
                if let Some(ref proof_info) = result.proof_nodes_info {
                    match proof_info.match_status {
                        MatchStatus::Exact => summary.proof_exact_matches += 1,
                        MatchStatus::Subset => summary.proof_subset_matches += 1,
                        MatchStatus::Superset => summary.proof_superset_matches += 1,
                        _ => {}
                    }
                }

                if let Some(ref inductive_info) = result.inductive_proof_nodes_info {
                    match inductive_info.match_status {
                        MatchStatus::Exact => summary.inductive_exact_matches += 1,
                        MatchStatus::Subset => summary.inductive_subset_matches += 1,
                        MatchStatus::Superset => summary.inductive_superset_matches += 1,
                        _ => {}
                    }
                }

                if passed {
                    summary.passed_tests += 1;
                } else {
                    summary.failed_tests += 1;
                    failed_tests.push(result);
                }

                if record_ops_match {
                    summary.record_ops_matches += 1;
                }
                if created_nodes_match {
                    summary.created_nodes_matches += 1;
                }
                if deleted_nodes_match {
                    summary.deleted_nodes_matches += 1;
                }
            }
            Err(e) => {
                summary.failed_tests += 1;
                eprintln!("Error running test {:?}: {}", test_path.file_name(), e);
            }
        }
    }

    // Print summary
    println!("\n=== MST Diff Suite Summary ===");
    println!("Total tests: {}", summary.total_tests);
    println!("Passed: {}", summary.passed_tests);
    println!("Failed: {}", summary.failed_tests);
    println!();
    println!(
        "Record ops matches: {}/{}",
        summary.record_ops_matches, summary.total_tests
    );
    println!(
        "Created nodes matches: {}/{}",
        summary.created_nodes_matches, summary.total_tests
    );
    println!(
        "Deleted nodes matches: {}/{}",
        summary.deleted_nodes_matches, summary.total_tests
    );
    println!();
    println!("Proof nodes (forward diff):");
    println!("  Exact: {}", summary.proof_exact_matches);
    println!(
        "  Subset (missing blocks): {}",
        summary.proof_subset_matches
    );
    println!(
        "  Superset (extra blocks): {}",
        summary.proof_superset_matches
    );
    println!();
    println!("Inductive proof nodes:");
    println!("  Exact: {}", summary.inductive_exact_matches);
    println!(
        "  Subset (missing blocks): {}",
        summary.inductive_subset_matches
    );
    println!(
        "  Superset (extra blocks): {}",
        summary.inductive_superset_matches
    );

    // Collect tests with missing inductive proof blocks
    let mut missing_block_cases = Vec::new();
    for test_path in &test_files {
        match run_test_case(test_path, suite_root).await {
            Ok(result) => {
                if let Some(ref info) = result.inductive_proof_nodes_info {
                    if matches!(info.match_status, MatchStatus::Subset) {
                        let missing: Vec<_> = info.expected.difference(&info.actual).cloned().collect();
                        missing_block_cases.push((result.test_name, missing));
                    }
                }
            }
            Err(_) => {}
        }
    }

    if !missing_block_cases.is_empty() {
        println!("\n=== CRITICAL: Tests Missing Inductive Proof Blocks ===");
        println!("Total cases missing blocks: {}", missing_block_cases.len());
        println!("\nFirst 10 cases:");
        for (test_name, missing) in missing_block_cases.iter().take(10) {
            println!("\n{}", test_name);
            println!("  Missing {} blocks:", missing.len());
            for cid in missing {
                println!("    {}", cid);
            }
        }
    }

    // Print first few failures for debugging
    if !failed_tests.is_empty() {
        println!("\n=== First 5 Failures (detailed) ===");
        for result in failed_tests.iter().take(5) {
            println!("\nTest: {}", result.test_name);
            println!("Description: {}", result.description);
            for error in &result.errors {
                println!("  - {}", error);
            }
        }

        println!("\n=== Failure Summary ===");
        println!("Total failures: {}", failed_tests.len());
    }

    // Assert all tests passed
    assert_eq!(
        summary.failed_tests, 0,
        "{} tests failed (see output above)",
        summary.failed_tests
    );
}
