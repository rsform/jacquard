//! Debug test for inspecting MST diff block tracking
//!
//! Loads a specific failing test case and shows exactly which blocks we compute
//! vs what's expected.

use jacquard_repo::mst::Mst;
use jacquard_repo::storage::MemoryBlockStore;
use jacquard_repo::car::parse_car_bytes;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use cid::Cid as IpldCid;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const TEST_SUITE_PATH: &str = "/home/orual/Git_Repos/mst-test-suite";

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
    mst_a: String,
    mst_b: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ExpectedResults {
    created_nodes: Vec<String>,
    deleted_nodes: Vec<String>,
    record_ops: Vec<serde_json::Value>,
    proof_nodes: Vec<String>,
    inductive_proof_nodes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    firehose_cids: Option<serde_json::Value>,
}

async fn load_car(path: &Path) -> anyhow::Result<(IpldCid, BTreeMap<IpldCid, Bytes>)> {
    let bytes = tokio::fs::read(path).await?;
    let parsed = parse_car_bytes(&bytes).await?;
    Ok((parsed.root, parsed.blocks))
}

fn cid_to_string(cid: &IpldCid) -> String {
    cid.to_string()
}

#[tokio::test]
#[ignore] // Local-only: requires mst-test-suite at /home/orual/Git_Repos/mst-test-suite
async fn debug_exhaustive_001_009() {
    let suite_root = Path::new(TEST_SUITE_PATH);
    let test_path = suite_root.join("tests/diff/exhaustive/exhaustive_001_009.json");

    // Load test case
    let test_json = tokio::fs::read_to_string(&test_path).await.unwrap();
    let test_case: MstDiffTestCase = serde_json::from_str(&test_json).unwrap();

    // Load CAR files
    let car_a_path = suite_root.join(&test_case.inputs.mst_a);
    let car_b_path = suite_root.join(&test_case.inputs.mst_b);

    let (root_a, blocks_a) = load_car(&car_a_path).await.unwrap();
    let (root_b, blocks_b) = load_car(&car_b_path).await.unwrap();

    // Create storage
    let mut all_blocks = blocks_a;
    all_blocks.extend(blocks_b);
    let storage = Arc::new(MemoryBlockStore::new_from_blocks(all_blocks));

    // Load MSTs
    let mst_a = Mst::load(storage.clone(), root_a, None);
    let mst_b = Mst::load(storage.clone(), root_b, None);

    // Compute diff
    let diff = mst_a.diff(&mst_b).await.unwrap();

    // Replicate create_commit's relevant_blocks logic
    let mut relevant_blocks = BTreeMap::new();
    let ops_count = diff.creates.len() + diff.updates.len() + diff.deletes.len();

    for (key, _cid) in &diff.creates {
        mst_b.blocks_for_path(key.as_str(), &mut relevant_blocks).await.unwrap();
        if ops_count > 1 {
            mst_a.blocks_for_path(key.as_str(), &mut relevant_blocks).await.unwrap();
        }
    }

    for (key, _new_cid, _old_cid) in &diff.updates {
        mst_b.blocks_for_path(key.as_str(), &mut relevant_blocks).await.unwrap();
    }

    for (key, _old_cid) in &diff.deletes {
        mst_b.blocks_for_path(key.as_str(), &mut relevant_blocks).await.unwrap();
    }

    // Filter out removed blocks before combining
    let removed_set: std::collections::HashSet<_> = diff.removed_mst_blocks.iter().copied().collect();
    let filtered_relevant: BTreeMap<_, _> = relevant_blocks
        .into_iter()
        .filter(|(cid, _)| !removed_set.contains(cid))
        .collect();

    let mut all_proof_blocks = diff.new_mst_blocks.clone();
    all_proof_blocks.extend(filtered_relevant);

    // Compare created_nodes
    let actual_created: BTreeSet<String> = diff
        .new_mst_blocks
        .keys()
        .map(cid_to_string)
        .collect();
    let expected_created: BTreeSet<String> = test_case
        .results
        .created_nodes
        .iter()
        .cloned()
        .collect();

    println!("\n=== Created Nodes ===");
    println!("Expected ({} blocks):", expected_created.len());
    for cid in &expected_created {
        println!("  {}", cid);
    }
    println!("\nActual ({} blocks):", actual_created.len());
    for cid in &actual_created {
        let marker = if expected_created.contains(cid) { " " } else { "* EXTRA" };
        println!("  {}{}", cid, marker);
    }

    // Compare deleted_nodes
    let actual_deleted: BTreeSet<String> = diff
        .removed_mst_blocks
        .iter()
        .map(cid_to_string)
        .collect();
    let expected_deleted: BTreeSet<String> = test_case
        .results
        .deleted_nodes
        .iter()
        .cloned()
        .collect();

    println!("\n=== Deleted Nodes ===");
    println!("Expected ({} blocks):", expected_deleted.len());
    for cid in &expected_deleted {
        println!("  {}", cid);
    }
    println!("\nActual ({} blocks):", actual_deleted.len());
    for cid in &actual_deleted {
        let marker = if expected_deleted.contains(cid) { " " } else { "* EXTRA" };
        println!("  {}{}", cid, marker);
    }

    // Show record operations
    println!("\n=== Record Operations ===");
    println!("Creates: {}", diff.creates.len());
    for (key, cid) in &diff.creates {
        println!("  CREATE {} -> {}", key, cid_to_string(cid));
    }
    println!("Updates: {}", diff.updates.len());
    for (key, new_cid, old_cid) in &diff.updates {
        println!("  UPDATE {} {} -> {}", key, cid_to_string(old_cid), cid_to_string(new_cid));
    }
    println!("Deletes: {}", diff.deletes.len());
    for (key, cid) in &diff.deletes {
        println!("  DELETE {} (was {})", key, cid_to_string(cid));
    }

    // Show proof nodes comparison
    println!("\n=== Proof Nodes (for reference) ===");
    println!("Expected proof_nodes ({} blocks):", test_case.results.proof_nodes.len());
    for cid in &test_case.results.proof_nodes {
        println!("  {}", cid);
    }

    println!("\nExpected inductive_proof_nodes ({} blocks):", test_case.results.inductive_proof_nodes.len());
    for cid in &test_case.results.inductive_proof_nodes {
        let marker = if test_case.results.proof_nodes.contains(cid) { " " } else { "* EXTRA for inductive" };
        println!("  {}{}", cid, marker);
    }

    println!("\n=== Our Computed Proof (all_proof_blocks) ===");
    let computed_proof: BTreeSet<String> = all_proof_blocks.keys().map(cid_to_string).collect();
    let expected_inductive: BTreeSet<String> = test_case.results.inductive_proof_nodes.iter().cloned().collect();

    println!("Computed ({} blocks):", computed_proof.len());
    for cid in &computed_proof {
        let marker = if expected_inductive.contains(cid) {
            ""
        } else {
            " * EXTRA (not in expected)"
        };
        println!("  {}{}", cid, marker);
    }

    println!("\nMissing from our computation:");
    for cid in &expected_inductive {
        if !computed_proof.contains(cid) {
            println!("  {} * MISSING", cid);
        }
    }

    // Don't fail the test, just show info
}
