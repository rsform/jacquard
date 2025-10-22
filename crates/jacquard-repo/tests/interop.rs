//! Interoperability tests using test vectors from atproto-interop-tests
//!
//! See: https://github.com/bluesky-social/atproto-interop-tests/tree/main/mst
//!

use std::sync::Arc;

use jacquard_common::types::crypto::SHA2_256;
use jacquard_repo::DAG_CBOR_CID_CODEC;
use jacquard_repo::mst::tree::{Mst, VerifiedWriteOp};
use jacquard_repo::mst::util::{common_prefix_len, layer_for_key};
use jacquard_repo::storage::BlockStore;
use jacquard_repo::storage::memory::MemoryBlockStore;
use rand::Rng;
use serde::Deserialize;

/// Test helper: Generate a random key at a specific layer
///
/// Reimplementation of gen_keys.py from atproto-interop-tests for Rust tests.
/// Generates keys like "A0/123456" that hash to a specific MST layer.
fn gen_key_at_layer(letter: char, layer: usize) -> String {
    let mut rng = rand::thread_rng();
    loop {
        let num: u32 = rng.gen_range(0..1_000_000);
        let key = format!("{}{}/{:06}", letter, layer, num);
        if layer_for_key(&key) == layer {
            return key;
        }
    }
}

#[derive(Debug, Deserialize)]
struct CommonPrefixTest {
    left: String,
    right: String,
    len: usize,
}

#[derive(Debug, Deserialize)]
struct KeyHeightTest {
    key: String,
    height: usize,
}

#[test]
fn test_common_prefix_interop() {
    let json = include_str!("fixtures/common_prefix.json");
    let tests: Vec<CommonPrefixTest> = serde_ipld_dagjson::from_slice(json.as_bytes()).unwrap();

    for test in tests {
        let result = common_prefix_len(&test.left, &test.right);
        assert_eq!(
            result, test.len,
            "common_prefix_len({:?}, {:?}) = {}, expected {}",
            test.left, test.right, result, test.len
        );
    }
}

#[test]
fn test_layer_for_key_interop() {
    let json = include_str!("fixtures/key_heights.json");
    let tests: Vec<KeyHeightTest> = serde_ipld_dagjson::from_slice(json.as_bytes()).unwrap();

    for test in tests {
        if test.key.is_empty() {
            // Empty key is invalid, skip
            continue;
        }

        let result = layer_for_key(&test.key);
        assert_eq!(
            result, test.height,
            "layer_for_key({:?}) = {}, expected {}",
            test.key, result, test.height
        );
    }
}

#[tokio::test]
async fn test_example_keys_tree_ops() {
    // Load example keys
    let keys_txt = include_str!("fixtures/example_keys.txt");
    let keys: Vec<&str> = keys_txt.lines().collect();

    let storage = Arc::new(MemoryBlockStore::new());
    let mut mst = Mst::new(storage);

    // Helper to create test CIDs
    fn test_cid(n: u8) -> cid::Cid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        cid::Cid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    // Add all keys
    for (i, &key) in keys.iter().enumerate() {
        mst = mst.add(key, test_cid(i as u8)).await.unwrap();
    }

    // Verify all keys can be retrieved
    for (i, &key) in keys.iter().enumerate() {
        let retrieved = mst.get(key).await.unwrap();
        assert_eq!(
            retrieved,
            Some(test_cid(i as u8)),
            "Failed to retrieve key: {}",
            key
        );
    }

    // Delete half the keys
    for (i, &key) in keys.iter().enumerate() {
        if i % 2 == 0 {
            mst = mst.delete(key).await.unwrap();
        }
    }

    // Verify deleted keys are gone and remaining keys still exist
    for (i, &key) in keys.iter().enumerate() {
        let retrieved = mst.get(key).await.unwrap();
        if i % 2 == 0 {
            assert_eq!(retrieved, None, "Key should be deleted: {}", key);
        } else {
            assert_eq!(
                retrieved,
                Some(test_cid(i as u8)),
                "Key should still exist: {}",
                key
            );
        }
    }
}

#[tokio::test]
async fn test_determinism_with_example_keys() {
    // Tree structure should be deterministic regardless of insertion order
    let keys_txt = include_str!("fixtures/example_keys.txt");
    let keys: Vec<&str> = keys_txt.lines().filter(|s| !s.is_empty()).collect();

    fn test_cid(n: u8) -> cid::Cid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        cid::Cid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    // Build tree in forward order
    let storage1 = Arc::new(MemoryBlockStore::new());
    let mut mst1 = Mst::new(storage1);
    for (i, &key) in keys.iter().enumerate() {
        mst1 = mst1.add(key, test_cid(i as u8)).await.unwrap();
    }

    // Build tree in reverse order
    let storage2 = Arc::new(MemoryBlockStore::new());
    let mut mst2 = Mst::new(storage2);
    for (i, &key) in keys.iter().rev().enumerate() {
        let idx = keys.len() - 1 - i;
        mst2 = mst2.add(key, test_cid(idx as u8)).await.unwrap();
    }

    // Check if all keys are retrievable from both trees
    let mut missing_in_1 = Vec::new();
    let mut missing_in_2 = Vec::new();

    for key in keys.iter() {
        let v1 = mst1.get(key).await.unwrap();
        let v2 = mst2.get(key).await.unwrap();

        if v1.is_none() {
            missing_in_1.push(key);
        }
        if v2.is_none() {
            missing_in_2.push(key);
        }
    }

    if !missing_in_1.is_empty() {
        eprintln!("Missing in mst1 ({} keys):", missing_in_1.len());
        for key in missing_in_1.iter().take(5) {
            eprintln!("  {}", key);
        }
    }

    if !missing_in_2.is_empty() {
        eprintln!("Missing in mst2 ({} keys):", missing_in_2.len());
        for key in missing_in_2.iter().take(5) {
            eprintln!("  {}", key);
        }
    }

    eprintln!("Keys missing in mst1: {}", missing_in_1.len());
    eprintln!("Keys missing in mst2: {}", missing_in_2.len());

    // Root CIDs should match
    eprintln!("mst1 root: {:?}", mst1.root().await.unwrap());
    eprintln!("mst2 root: {:?}", mst2.root().await.unwrap());

    assert_eq!(
        mst1.root().await.unwrap(),
        mst2.root().await.unwrap(),
        "Tree structure should be deterministic"
    );
}

#[tokio::test]
async fn test_generated_keys_at_specific_layers() {
    // Generate keys at different layers and verify they work correctly
    let storage = Arc::new(MemoryBlockStore::new());
    let mut mst = Mst::new(storage);

    fn test_cid(n: u8) -> cid::Cid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        cid::Cid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    // Generate keys at layers 0-5
    let mut keys_by_layer: Vec<(String, usize)> = Vec::new();
    for layer in 0..=5 {
        let key = gen_key_at_layer('T', layer);
        // Verify it's actually at the expected layer
        assert_eq!(layer_for_key(&key), layer);
        keys_by_layer.push((key, layer));
    }

    // Add all keys to tree
    for (i, (key, _layer)) in keys_by_layer.iter().enumerate() {
        mst = mst.add(key, test_cid(i as u8)).await.unwrap();
    }

    // Verify all keys can be retrieved
    for (i, (key, _layer)) in keys_by_layer.iter().enumerate() {
        let retrieved = mst.get(key).await.unwrap();
        assert_eq!(retrieved, Some(test_cid(i as u8)));
    }
}

#[derive(Debug, Deserialize)]
struct CommitProofFixture {
    comment: String,
    #[serde(rename = "leafValue")]
    leaf_value: String,
    keys: Vec<String>,
    adds: Vec<String>,
    dels: Vec<String>,
    #[serde(rename = "rootBeforeCommit")]
    root_before_commit: String,
    #[serde(rename = "rootAfterCommit")]
    root_after_commit: String,
    #[serde(rename = "blocksInProof")]
    _blocks_in_proof: Vec<String>,
}

#[tokio::test]
async fn test_commit_proof_fixtures() {
    let json = include_str!("fixtures/commit_proof.json");
    let fixtures: Vec<CommitProofFixture> =
        serde_ipld_dagjson::from_slice(json.as_bytes()).unwrap();

    for fixture in fixtures {
        println!("\n=== Testing: {} ===", fixture.comment);

        // Parse the leaf value CID
        let leaf_cid: cid::Cid = fixture.leaf_value.parse().unwrap();

        // Build initial tree from keys
        let storage = Arc::new(MemoryBlockStore::new());
        let mut mst = Mst::new(storage);

        for key in &fixture.keys {
            mst = mst.add(key, leaf_cid).await.unwrap();
        }

        // Verify root before commit
        let root_before = mst.root().await.unwrap();
        let expected_before: cid::Cid = fixture.root_before_commit.parse().unwrap();

        assert_eq!(
            root_before.to_string(),
            expected_before.to_string(),
            "Root CID mismatch before commit (fixture: {})",
            fixture.comment
        );

        // Apply adds
        for key in &fixture.adds {
            mst = mst.add(key, leaf_cid).await.unwrap();
        }

        // Apply deletes
        for key in &fixture.dels {
            mst = mst.delete(key).await.unwrap();
        }

        // Verify root after commit
        let root_after = mst.root().await.unwrap();
        let expected_after: cid::Cid = fixture.root_after_commit.parse().unwrap();

        assert_eq!(
            root_after.to_string(),
            expected_after.to_string(),
            "Root CID mismatch after commit (fixture: {})",
            fixture.comment
        );

        println!("✓ Passed: {}", fixture.comment);
    }
}

#[tokio::test]
async fn test_commit_proof_using_batch() {
    // Same as above but using batch operations instead of individual add/delete
    let json = include_str!("fixtures/commit_proof.json");
    let fixtures: Vec<CommitProofFixture> =
        serde_ipld_dagjson::from_slice(json.as_bytes()).unwrap();

    for fixture in fixtures {
        println!("\n=== Testing (batch): {} ===", fixture.comment);

        let leaf_cid: cid::Cid = fixture.leaf_value.parse().unwrap();

        // Build initial tree
        let storage = Arc::new(MemoryBlockStore::new());
        let mut mst = Mst::new(storage);

        for key in &fixture.keys {
            mst = mst.add(key, leaf_cid).await.unwrap();
        }

        // Verify before state
        let root_before = mst.root().await.unwrap();
        let expected_before: cid::Cid = fixture.root_before_commit.parse().unwrap();
        assert_eq!(root_before.to_string(), expected_before.to_string());

        // Build batch operations
        use smol_str::SmolStr;

        let mut ops = Vec::new();

        // Note: adds in commit fixtures might include keys that already exist
        // In that case we should use Update instead of Create
        for key in &fixture.adds {
            // Check if key already exists
            if mst.get(key).await.unwrap().is_some() {
                // Update existing key
                ops.push(VerifiedWriteOp::Update {
                    key: SmolStr::new(key),
                    cid: leaf_cid,
                    prev: leaf_cid, // Same CID since we're using uniform leaf values
                });
            } else {
                // Create new key
                ops.push(VerifiedWriteOp::Create {
                    key: SmolStr::new(key),
                    cid: leaf_cid,
                });
            }
        }

        for key in &fixture.dels {
            ops.push(VerifiedWriteOp::Delete {
                key: SmolStr::new(key),
                prev: leaf_cid, // We know the value from the fixture
            });
        }

        // Apply batch
        mst = mst.batch(&ops).await.unwrap();

        // Verify after state
        let root_after = mst.root().await.unwrap();
        let expected_after: cid::Cid = fixture.root_after_commit.parse().unwrap();

        assert_eq!(
            root_after.to_string(),
            expected_after.to_string(),
            "Root CID mismatch after batch ops (fixture: {})",
            fixture.comment
        );

        println!("✓ Passed (batch): {}", fixture.comment);
    }
}

#[tokio::test]
async fn test_commit_proof_diff_validation() {
    // Verify that diff calculation matches the expected adds/dels from fixtures
    let json = include_str!("fixtures/commit_proof.json");
    let fixtures: Vec<CommitProofFixture> =
        serde_ipld_dagjson::from_slice(json.as_bytes()).unwrap();

    for fixture in fixtures {
        println!("\n=== Testing diff: {} ===", fixture.comment);

        let leaf_cid: cid::Cid = fixture.leaf_value.parse().unwrap();

        // Build "before" tree
        let storage_before = Arc::new(MemoryBlockStore::new());
        let mut mst_before = Mst::new(storage_before);
        for key in &fixture.keys {
            mst_before = mst_before.add(key, leaf_cid).await.unwrap();
        }

        // Build "after" tree
        let storage_after = Arc::new(MemoryBlockStore::new());
        let mut mst_after = Mst::new(storage_after);

        // Start with same keys
        for key in &fixture.keys {
            mst_after = mst_after.add(key, leaf_cid).await.unwrap();
        }

        // Apply ops to after tree
        for key in &fixture.adds {
            mst_after = mst_after.add(key, leaf_cid).await.unwrap();
        }
        for key in &fixture.dels {
            mst_after = mst_after.delete(key).await.unwrap();
        }

        // Compute diff
        let diff = mst_before.diff(&mst_after).await.unwrap();

        // Verify diff matches expected operations
        println!(
            "  Diff: {} creates, {} updates, {} deletes",
            diff.creates.len(),
            diff.updates.len(),
            diff.deletes.len()
        );
        println!(
            "  Expected: {} adds, {} dels",
            fixture.adds.len(),
            fixture.dels.len()
        );

        // Creates should match adds (keys not in original tree)
        let added_keys: std::collections::HashSet<_> =
            fixture.adds.iter().map(|s| s.as_str()).collect();
        let _deleted_keys: std::collections::HashSet<_> =
            fixture.dels.iter().map(|s| s.as_str()).collect();
        let original_keys: std::collections::HashSet<_> =
            fixture.keys.iter().map(|s| s.as_str()).collect();

        // Compute expected creates (adds that weren't in original)
        let expected_creates: Vec<_> = added_keys.difference(&original_keys).map(|s| *s).collect();

        // Compute expected updates (adds that WERE in original - replacing same CID)
        let expected_updates: Vec<_> = added_keys
            .intersection(&original_keys)
            .map(|s| *s)
            .collect();

        println!("  Expected creates: {}", expected_creates.len());
        println!("  Expected updates: {}", expected_updates.len());

        // Total ops should match
        let total_diff_ops = diff.creates.len() + diff.updates.len() + diff.deletes.len();
        let total_expected_ops = fixture.adds.len() + fixture.dels.len();

        assert_eq!(
            total_diff_ops, total_expected_ops,
            "Total operations mismatch in diff (fixture: {})",
            fixture.comment
        );

        println!("✓ Passed diff: {}", fixture.comment);
    }
}

#[tokio::test]
async fn test_commit_proof_incremental_cids() {
    // Show CID after each key insertion to find where we diverge
    let json = include_str!("fixtures/commit_proof.json");
    let fixtures: Vec<CommitProofFixture> =
        serde_ipld_dagjson::from_slice(json.as_bytes()).unwrap();

    let fixture = &fixtures[0]; // "two deep split"
    println!("\n=== {} ===", fixture.comment);
    println!("Expected final CID: {}", fixture.root_before_commit);

    let leaf_cid: cid::Cid = fixture.leaf_value.parse().unwrap();
    println!("Leaf value CID: {}", leaf_cid);

    let storage = Arc::new(MemoryBlockStore::new());
    let mut mst = Mst::new(storage);

    for (i, key) in fixture.keys.iter().enumerate() {
        mst = mst.add(key, leaf_cid).await.unwrap();
        let root = mst.root().await.unwrap();
        println!("After adding key {}: {} -> root CID: {}", i, key, root);
    }

    println!("\nFinal root CID: {}", mst.root().await.unwrap());
    println!("Expected:       {}", fixture.root_before_commit);
}

#[tokio::test]
async fn test_rsky_simple_case() {
    // From rsky's "handle_new_layers_that_are_two_higher_than_existing" test
    // Simple case: 2 keys at layer 0
    let cid1: cid::Cid = "bafyreie5cvv4h45feadgeuwhbcutmh6t2ceseocckahdoe6uat64zmz454"
        .parse()
        .unwrap();
    let storage = Arc::new(MemoryBlockStore::new());
    let mut mst = Mst::new(storage);

    // Add A (layer 0)
    mst = mst
        .add("com.example.record/3jqfcqzm3ft2j", cid1)
        .await
        .unwrap();
    println!("After A: {}", mst.root().await.unwrap());

    // Add C (layer 0)
    mst = mst
        .add("com.example.record/3jqfcqzm3fz2j", cid1)
        .await
        .unwrap();

    let root = mst.root().await.unwrap();
    let expected = "bafyreidfcktqnfmykz2ps3dbul35pepleq7kvv526g47xahuz3rqtptmky";

    println!("Our CID:      {}", root);
    println!("Expected CID: {}", expected);

    assert_eq!(
        root.to_string(),
        expected,
        "CID mismatch for simple 2-key tree"
    );
}

#[tokio::test]
async fn test_real_repo_car_roundtrip() {
    use jacquard_repo::car::{read_car, write_car};
    use std::path::Path;
    use tempfile::NamedTempFile;

    // Check if fixture exists (local only - not in CI)
    let fixture_path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/repo-nonbinary.computer-2025-10-21T13_05_55.090Z.car"
    ));

    if !fixture_path.exists() {
        eprintln!("⚠️  Skipping test_real_repo_car_roundtrip - fixture not present");
        eprintln!("   (This is expected in CI - test only runs locally)");
        return;
    }

    println!("✓ Found real repo CAR fixture");

    // Read the CAR file
    let blocks = read_car(fixture_path)
        .await
        .expect("Failed to read CAR file");
    println!("✓ Loaded {} blocks from CAR", blocks.len());

    assert!(!blocks.is_empty(), "CAR file should contain blocks");

    // Write to a temp file
    let temp_file = NamedTempFile::new().unwrap();

    // Note: We can't easily extract the original roots without parsing the CAR header
    // For now, just use the first block's CID as the root (if it exists)
    let roots: Vec<_> = blocks.keys().take(1).copied().collect();

    write_car(temp_file.path(), roots.clone(), blocks.clone())
        .await
        .expect("Failed to write CAR file");
    println!("✓ Wrote CAR to temp file");

    // Read it back
    let blocks_roundtrip = read_car(temp_file.path())
        .await
        .expect("Failed to read roundtrip CAR");
    println!(
        "✓ Read {} blocks from roundtrip CAR",
        blocks_roundtrip.len()
    );

    // Verify all blocks match
    assert_eq!(
        blocks.len(),
        blocks_roundtrip.len(),
        "Block count mismatch after roundtrip"
    );

    for (cid, data) in &blocks {
        let roundtrip_data = blocks_roundtrip
            .get(cid)
            .expect(&format!("Missing block after roundtrip: {}", cid));
        assert_eq!(data, roundtrip_data, "Block data mismatch for CID: {}", cid);
    }

    println!("✓ All {} blocks match after roundtrip", blocks.len());
}

#[tokio::test]
async fn test_real_repo_car_streaming() {
    use jacquard_repo::car::stream_car;
    use std::path::Path;

    let fixture_path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/repo-nonbinary.computer-2025-10-21T13_05_55.090Z.car"
    ));

    if !fixture_path.exists() {
        eprintln!("⚠️  Skipping test_real_repo_car_streaming - fixture not present");
        return;
    }

    let mut stream = stream_car(fixture_path)
        .await
        .expect("Failed to create CAR stream");

    println!("✓ Created CAR stream");
    println!("  Roots: {:?}", stream.roots());

    let mut block_count = 0;
    while let Some((cid, data)) = stream.next().await.expect("Stream error") {
        block_count += 1;
        if block_count <= 5 {
            println!("  Block {}: {} ({} bytes)", block_count, cid, data.len());
        }
    }

    println!("✓ Streamed {} blocks total", block_count);
    assert!(block_count > 0, "Should have streamed at least one block");
}

#[tokio::test]
async fn test_real_repo_mst_structure() {
    use jacquard_repo::car::read_car;
    use jacquard_repo::mst::tree::Mst;
    use jacquard_repo::storage::memory::MemoryBlockStore;
    use std::path::Path;

    let fixture_path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/repo-nonbinary.computer-2025-10-21T13_05_55.090Z.car"
    ));

    if !fixture_path.exists() {
        eprintln!("⚠️  Skipping test_real_repo_mst_structure - fixture not present");
        return;
    }

    println!("✓ Loading real repo CAR file");

    // Read CAR and load into storage
    let blocks = read_car(fixture_path).await.expect("Failed to read CAR");
    println!("✓ Loaded {} blocks", blocks.len());

    let storage = Arc::new(MemoryBlockStore::new());

    // Load all blocks into storage
    let mut block_vec = Vec::new();
    for (cid, data) in blocks.iter() {
        block_vec.push((*cid, data.clone()));
    }
    storage
        .put_many(block_vec)
        .await
        .expect("Failed to store blocks");
    println!("✓ Loaded all blocks into storage");

    // Get roots from CAR header
    let roots = jacquard_repo::car::read_car_header(fixture_path)
        .await
        .expect("Failed to read header");

    assert!(!roots.is_empty(), "CAR should have at least one root");
    let commit_cid = roots[0];
    println!("✓ Commit CID: {}", commit_cid);

    // Parse commit to get MST root
    #[derive(serde::Deserialize)]
    struct Commit {
        data: cid::Cid,
        // We only care about the data field (MST root)
    }

    let commit_bytes = storage
        .get(&commit_cid)
        .await
        .expect("Failed to get commit")
        .expect("Commit not found");

    let commit: Commit =
        serde_ipld_dagcbor::from_slice(&commit_bytes).expect("Failed to parse commit");

    let mst_root = commit.data;
    println!("✓ MST root CID: {}", mst_root);

    // Load MST
    let mst = Mst::load(storage.clone(), mst_root, None);
    println!("✓ Loaded MST from storage");

    // Verify we can get the root CID
    let root_cid = mst.root().await.expect("Failed to get root CID");
    assert_eq!(root_cid, mst_root, "MST root CID should match");
    println!("✓ MST root CID matches");

    // Get all leaves to verify tree structure
    let leaves = mst.leaves().await.expect("Failed to get leaves");
    println!("✓ MST contains {} leaf entries", leaves.len());

    assert!(!leaves.is_empty(), "MST should have at least one leaf");

    // Verify leaves are in lexicographic order
    for i in 1..leaves.len() {
        let prev_key = &leaves[i - 1].0;
        let curr_key = &leaves[i].0;
        assert!(
            prev_key < curr_key,
            "Leaves should be in lexicographic order: {:?} >= {:?}",
            prev_key,
            curr_key
        );
    }
    println!("✓ All leaves are in lexicographic order");

    // Test get operation on first few keys
    for (i, (key, expected_cid)) in leaves.iter().take(10).enumerate() {
        let retrieved = mst.get(key).await.expect("Failed to get key");
        assert_eq!(
            retrieved,
            Some(*expected_cid),
            "Get operation failed for key {}: {}",
            i,
            key
        );
    }
    println!("✓ Get operations work correctly on sampled keys");

    // Verify all leaves are retrievable via get
    println!("  Verifying all {} keys are retrievable...", leaves.len());
    for (key, expected_cid) in &leaves {
        let retrieved = mst.get(key).await.expect("Failed to get key");
        assert_eq!(
            retrieved,
            Some(*expected_cid),
            "Get operation failed for key: {}",
            key
        );
    }
    println!("✓ All {} keys are retrievable via get()", leaves.len());
}

#[tokio::test]
async fn test_real_repo_mst_operations() {
    use jacquard_repo::car::read_car;
    use jacquard_repo::mst::tree::Mst;
    use jacquard_repo::storage::memory::MemoryBlockStore;
    use std::path::Path;

    let fixture_path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/repo-nonbinary.computer-2025-10-21T13_05_55.090Z.car"
    ));

    if !fixture_path.exists() {
        eprintln!("⚠️  Skipping test_real_repo_mst_operations - fixture not present");
        return;
    }

    // Load CAR and set up storage
    let blocks = read_car(fixture_path).await.expect("Failed to read CAR");
    let storage = Arc::new(MemoryBlockStore::new());

    let mut block_vec = Vec::new();
    for (cid, data) in blocks.iter() {
        block_vec.push((*cid, data.clone()));
    }
    storage
        .put_many(block_vec)
        .await
        .expect("Failed to store blocks");

    // Get MST root
    let roots = jacquard_repo::car::read_car_header(fixture_path)
        .await
        .expect("Failed to read header");
    let commit_cid = roots[0];

    #[derive(serde::Deserialize)]
    struct Commit {
        data: cid::Cid,
    }

    let commit_bytes = storage.get(&commit_cid).await.unwrap().unwrap();
    let commit: Commit = serde_ipld_dagcbor::from_slice(&commit_bytes).unwrap();
    let mst_root = commit.data;

    // Load original MST
    let original_mst = Mst::load(storage.clone(), mst_root, None);
    let original_leaves = original_mst.leaves().await.expect("Failed to get leaves");
    println!("✓ Loaded MST with {} leaves", original_leaves.len());

    // Test adding a new key
    fn test_cid(n: u8) -> cid::Cid {
        let data = vec![n; 32];
        let mh = multihash::Multihash::wrap(SHA2_256, &data).unwrap();
        cid::Cid::new_v1(DAG_CBOR_CID_CODEC, mh)
    }

    let new_key = "app.bsky.feed.post/zzztestkey123";
    let modified_mst = original_mst
        .add(new_key, test_cid(99))
        .await
        .expect("Failed to add key");

    // Verify new key exists
    assert_eq!(
        modified_mst.get(new_key).await.unwrap(),
        Some(test_cid(99)),
        "New key should be retrievable"
    );
    println!("✓ Successfully added new key to MST");

    // Verify old keys still exist
    for (key, cid) in original_leaves.iter().take(10) {
        assert_eq!(
            modified_mst.get(key).await.unwrap(),
            Some(*cid),
            "Original keys should still be retrievable"
        );
    }
    println!("✓ Original keys still retrievable after add");

    // Test that modified MST has one more leaf
    let modified_leaves = modified_mst.leaves().await.unwrap();
    assert_eq!(
        modified_leaves.len(),
        original_leaves.len() + 1,
        "Modified MST should have one more leaf"
    );
    println!("✓ Modified MST has correct leaf count");

    // Test deleting a key
    if let Some((key_to_delete, _)) = original_leaves.first() {
        let mst_after_delete = modified_mst
            .delete(key_to_delete)
            .await
            .expect("Failed to delete key");

        assert_eq!(
            mst_after_delete.get(key_to_delete).await.unwrap(),
            None,
            "Deleted key should not be retrievable"
        );
        println!("✓ Successfully deleted key from MST");

        // Verify other keys still exist
        for (key, cid) in original_leaves.iter().skip(1).take(10) {
            assert_eq!(
                mst_after_delete.get(key).await.unwrap(),
                Some(*cid),
                "Other keys should still be retrievable after delete"
            );
        }
        println!("✓ Other keys still retrievable after delete");
    }
}

#[tokio::test]
async fn test_real_repo_mst_determinism() {
    use jacquard_repo::car::read_car;
    use jacquard_repo::mst::tree::Mst;
    use jacquard_repo::storage::memory::MemoryBlockStore;
    use std::path::Path;

    let fixture_path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/repo-nonbinary.computer-2025-10-21T13_05_55.090Z.car"
    ));

    if !fixture_path.exists() {
        eprintln!("⚠️  Skipping test_real_repo_mst_determinism - fixture not present");
        return;
    }

    // Load CAR and set up storage
    let blocks = read_car(fixture_path).await.expect("Failed to read CAR");
    let storage = Arc::new(MemoryBlockStore::new());

    storage
        .put_many(blocks)
        .await
        .expect("Failed to store blocks");

    // Get MST root and leaves
    let roots = jacquard_repo::car::read_car_header(fixture_path)
        .await
        .expect("Failed to read header");
    let commit_cid = roots[0];

    #[derive(serde::Deserialize)]
    struct Commit {
        data: cid::Cid,
    }

    let commit_bytes = storage.get(&commit_cid).await.unwrap().unwrap();
    let commit: Commit = serde_ipld_dagcbor::from_slice(&commit_bytes).unwrap();
    let original_mst_root = commit.data;

    let original_mst = Mst::load(storage.clone(), original_mst_root, None);
    let leaves = original_mst.leaves().await.expect("Failed to get leaves");
    println!(
        "✓ Loaded MST with {} leaves for determinism test",
        leaves.len()
    );

    // Take first 100 keys and rebuild tree in different order
    let test_leaves: Vec<_> = leaves.iter().take(100).cloned().collect();
    println!("  Testing determinism with {} keys", test_leaves.len());

    // Build tree in original order
    let storage1 = Arc::new(MemoryBlockStore::new());
    let mut mst1 = Mst::new(storage1);
    for (key, cid) in &test_leaves {
        mst1 = mst1.add(key, *cid).await.unwrap();
    }
    let cid1 = mst1.root().await.unwrap();

    // Build tree in reverse order
    let storage2 = Arc::new(MemoryBlockStore::new());
    let mut mst2 = Mst::new(storage2);
    for (key, cid) in test_leaves.iter().rev() {
        mst2 = mst2.add(key, *cid).await.unwrap();
    }
    let cid2 = mst2.root().await.unwrap();

    println!("  MST1 root: {}", cid1);
    println!("  MST2 root: {}", cid2);

    // Verify all keys are present in both trees
    for (key, expected_cid) in &test_leaves {
        let v1 = mst1.get(key).await.unwrap();
        let v2 = mst2.get(key).await.unwrap();

        assert_eq!(
            v1,
            Some(*expected_cid),
            "Key should be retrievable from mst1: {}",
            key
        );
        assert_eq!(
            v2,
            Some(*expected_cid),
            "Key should be retrievable from mst2: {}",
            key
        );
    }
    println!("✓ All keys retrievable from both trees");

    // Check if root CIDs match (determinism test)
    assert_eq!(
        cid1, cid2,
        "Tree structure must be deterministic - root CIDs should match"
    );
    println!("✓ Root CIDs match - tree structure is deterministic!");
}
