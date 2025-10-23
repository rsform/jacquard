//! Stress tests for firehose commit validation
//!
//! Generates thousands of random operations to catch edge cases in v1.1 validation.

use jacquard_common::IntoStatic;
use jacquard_common::types::crypto::{KeyCodec, PublicKey};
use jacquard_common::types::recordkey::Rkey;
use jacquard_common::types::string::{Datetime, Did, Nsid, RecordKey};
use jacquard_common::types::tid::Ticker;
use jacquard_common::types::value::RawData;
use jacquard_repo::Repository;
use jacquard_repo::car::read_car_header;
use jacquard_repo::mst::RecordWriteOp;
use jacquard_repo::storage::{BlockStore, MemoryBlockStore};
use rand::Rng;
use rand::seq::SliceRandom;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

// Test configuration
const INITIAL_RECORDS: usize = 50;
const STRESS_OPERATIONS: usize = 100;
const BATCH_SIZE_RANGE: (usize, usize) = (1, 10);

fn make_test_record(n: u32, text: &str) -> BTreeMap<SmolStr, RawData<'static>> {
    let mut record = BTreeMap::new();
    record.insert(
        SmolStr::new("$type"),
        RawData::String("app.bsky.feed.post".into()),
    );
    record.insert(
        SmolStr::new("text"),
        RawData::String(format!("{} #{}", text, n).into()),
    );
    record.insert(
        SmolStr::new("createdAt"),
        RawData::String("2024-01-01T00:00:00Z".to_string().into()),
    );
    record
}

fn get_public_key(signing_key: &k256::ecdsa::SigningKey) -> PublicKey<'static> {
    let verifying_key = signing_key.verifying_key();
    let pubkey_bytes = verifying_key.to_encoded_point(true).as_bytes().to_vec();
    PublicKey {
        codec: KeyCodec::Secp256k1,
        bytes: pubkey_bytes.into(),
    }
}

async fn create_test_repo(storage: Arc<MemoryBlockStore>) -> Repository<MemoryBlockStore> {
    let did = Did::new("did:plc:stresstest").unwrap();
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);

    Repository::create(storage, did.into_static(), &signing_key, None)
        .await
        .unwrap()
}

/// Track existing records for generating realistic updates/deletes
struct RecordTracker {
    records: HashMap<String, u32>,
    ticker: Ticker,
}

impl RecordTracker {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
            ticker: Ticker::new(),
        }
    }

    fn gen_new_rkey(&mut self) -> String {
        self.ticker.next(None).into_static().to_string()
    }

    fn pick_random_existing<R: Rng>(&self, rng: &mut R) -> Option<String> {
        let keys: Vec<_> = self.records.keys().cloned().collect();
        keys.choose(rng).cloned()
    }

    fn add(&mut self, rkey: String, counter: u32) {
        self.records.insert(rkey, counter);
    }

    fn remove(&mut self, rkey: &str) {
        self.records.remove(rkey);
    }

    fn len(&self) -> usize {
        self.records.len()
    }
}

#[derive(Debug, Clone)]
enum TestOp {
    Create { rkey: String, counter: u32 },
    Update { rkey: String, counter: u32 },
    Delete { rkey: String },
}

fn generate_creates_only<R: Rng>(
    rng: &mut R,
    tracker: &mut RecordTracker,
    count: usize,
) -> Vec<TestOp> {
    let mut ops = Vec::new();
    for _ in 0..count {
        let rkey = tracker.gen_new_rkey();
        let counter: u32 = rng.r#gen();
        tracker.add(rkey.clone(), counter);
        ops.push(TestOp::Create { rkey, counter });
    }
    ops
}

fn generate_random_ops<R: Rng>(
    rng: &mut R,
    tracker: &mut RecordTracker,
    count: usize,
) -> Vec<TestOp> {
    let mut ops = Vec::new();

    for _ in 0..count {
        // Weighted random choice: 50% create, 30% update, 20% delete
        let action = rng.gen_range(0..100);

        let op = if action < 50 || tracker.len() == 0 {
            // Create
            let rkey = tracker.gen_new_rkey();
            let counter: u32 = rng.r#gen();
            tracker.add(rkey.clone(), counter);
            TestOp::Create { rkey, counter }
        } else if action < 80 {
            // Update
            if let Some(rkey) = tracker.pick_random_existing(rng) {
                let counter: u32 = rng.r#gen();
                tracker.add(rkey.clone(), counter);
                TestOp::Update { rkey, counter }
            } else {
                // Fall back to create if no records exist
                let rkey = tracker.gen_new_rkey();
                let counter: u32 = rng.r#gen();
                tracker.add(rkey.clone(), counter);
                TestOp::Create { rkey, counter }
            }
        } else {
            // Delete
            if let Some(rkey) = tracker.pick_random_existing(rng) {
                tracker.remove(&rkey);
                TestOp::Delete { rkey }
            } else {
                // Fall back to create if no records exist
                let rkey = tracker.gen_new_rkey();
                let counter: u32 = rng.r#gen();
                tracker.add(rkey.clone(), counter);
                TestOp::Create { rkey, counter }
            }
        };

        ops.push(op);
    }

    ops
}

fn test_ops_to_record_writes(ops: Vec<TestOp>, collection: &Nsid) -> Vec<RecordWriteOp<'static>> {
    let collection_static = collection.clone().into_static();
    ops.into_iter()
        .map(|op| match op {
            TestOp::Create { rkey, counter } => RecordWriteOp::Create {
                collection: collection_static.clone(),
                rkey: RecordKey(Rkey::new(&rkey).unwrap()).into_static(),
                record: make_test_record(counter, "Random post"),
            },
            TestOp::Update { rkey, counter } => RecordWriteOp::Update {
                collection: collection_static.clone(),
                rkey: RecordKey(Rkey::new(&rkey).unwrap()).into_static(),
                record: make_test_record(counter, "Updated post"),
                prev: None,
            },
            TestOp::Delete { rkey } => RecordWriteOp::Delete {
                collection: collection_static.clone(),
                rkey: RecordKey(Rkey::new(&rkey).unwrap()).into_static(),
                prev: None,
            },
        })
        .collect()
}

#[tokio::test]
async fn test_stress_random_operations() {
    let storage = Arc::new(MemoryBlockStore::new());
    let mut repo = create_test_repo(storage.clone()).await;

    let collection = Nsid::new("app.bsky.feed.post").unwrap();
    let did = Did::new("did:plc:stresstest").unwrap();
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
    let pubkey = get_public_key(&signing_key);

    let mut rng = rand::thread_rng();
    let mut tracker = RecordTracker::new();

    // Step 1: Create initial batch of records
    println!("Creating {} initial records...", INITIAL_RECORDS);
    println!("Repo before initial commit:\n{}", repo);

    let initial_ops = generate_creates_only(&mut rng, &mut tracker, INITIAL_RECORDS);
    let record_writes = test_ops_to_record_writes(initial_ops, &collection);

    let (repo_ops, commit_data) = repo
        .create_commit(&record_writes, &did, None, &signing_key)
        .await
        .unwrap();

    repo.apply_commit(commit_data.clone()).await.unwrap();
    println!("Repo after initial commit:\n{}", repo);

    // Validate initial commit
    let firehose_commit = commit_data
        .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
        .await
        .unwrap();

    firehose_commit
        .validate_v1_1(&pubkey)
        .await
        .expect("Initial batch should validate");

    println!(
        "Initial repo created with {} records",
        tracker.records.len()
    );

    // Step 2: Generate and apply random operations in batches
    let mut commit_count = 1;
    let mut total_ops = 0;

    while total_ops < STRESS_OPERATIONS {
        let batch_size = rng.gen_range(BATCH_SIZE_RANGE.0..=BATCH_SIZE_RANGE.1);
        let remaining = STRESS_OPERATIONS - total_ops;
        let ops_count = batch_size.min(remaining);

        let ops = generate_random_ops(&mut rng, &mut tracker, ops_count);
        let record_writes = test_ops_to_record_writes(ops, &collection);

        let (repo_ops, commit_data) = repo
            .create_commit(&record_writes, &did, None, &signing_key)
            .await
            .unwrap();

        repo.apply_commit(commit_data.clone()).await.unwrap();

        // Validate firehose commit
        commit_count += 1;
        let firehose_commit = commit_data
            .to_firehose_commit(
                &did,
                commit_count,
                Datetime::now(),
                repo_ops.clone(),
                vec![],
            )
            .await
            .unwrap();

        firehose_commit
            .validate_v1_1(&pubkey)
            .await
            .unwrap_or_else(|e| {
                eprintln!(
                    "Validation failed at commit {} (batch size {})",
                    commit_count, ops_count
                );
                eprintln!("Error: {}", e);
                eprintln!("Operations:\n{:?}", repo_ops);
                eprintln!("Relevant blocks:\n{:?}", commit_data.relevant_blocks.keys());
                eprintln!("All blocks:\n{:?}", commit_data.blocks.keys());
                panic!(
                    "Validation failed at commit {} (batch size {}): {}",
                    commit_count, ops_count, e
                )
            });

        total_ops += ops_count;

        if commit_count % 50 == 0 {
            println!(
                "Processed {} commits, {} total operations, {} records in repo",
                commit_count,
                total_ops,
                tracker.records.len()
            );
        }
    }

    println!(
        "Stress test complete: {} commits, {} operations, {} final records",
        commit_count,
        total_ops,
        tracker.records.len()
    );
}

#[tokio::test]
async fn test_stress_large_batches() {
    let storage = Arc::new(MemoryBlockStore::new());
    let mut repo = create_test_repo(storage.clone()).await;

    let collection = Nsid::new("app.bsky.feed.post").unwrap();
    let did = Did::new("did:plc:stresstest").unwrap();
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
    let pubkey = get_public_key(&signing_key);

    let mut rng = rand::thread_rng();
    let mut tracker = RecordTracker::new();

    // Create initial records
    let initial_ops = generate_creates_only(&mut rng, &mut tracker, 100);
    let record_writes = test_ops_to_record_writes(initial_ops, &collection);
    let (repo_ops, commit_data) = repo
        .create_commit(
            &record_writes,
            &did,
            Some(repo.current_commit_cid().clone()),
            &signing_key,
        )
        .await
        .unwrap();
    repo.apply_commit(commit_data.clone()).await.unwrap();

    let firehose_commit = commit_data
        .to_firehose_commit(&did, 1, Datetime::now(), repo_ops, vec![])
        .await
        .unwrap();

    firehose_commit.validate_v1_1(&pubkey).await.unwrap();

    for batch_num in 1..=5000 {
        let batch_size = rng.gen_range(1..=20);
        let ops = generate_random_ops(&mut rng, &mut tracker, batch_size);
        let record_writes = test_ops_to_record_writes(ops, &collection);

        let (repo_ops, commit_data) = repo
            .create_commit(&record_writes, &did, None, &signing_key)
            .await
            .unwrap();

        repo.apply_commit(commit_data.clone()).await.unwrap();

        let firehose_commit = commit_data
            .to_firehose_commit(&did, batch_num + 1, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        firehose_commit
            .validate_v1_1(&pubkey)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "Large batch validation failed (batch size {}): {}",
                    batch_size, e
                )
            });

        repo.apply_commit(commit_data).await.unwrap();
        // println!(
        //     "Validated large batch {} with {} ops",
        //     batch_num, batch_size
        // );
    }
}

#[tokio::test]
async fn test_stress_with_fixture() {
    use jacquard_repo::car::read_car;
    use std::path::Path;
    let fixture_path =
        Path::new("tests/fixtures/repo-nonbinary.computer-2025-10-21T13_05_55.090Z.car");

    // Skip test in CI if fixture doesn't exist
    if !fixture_path.exists() {
        println!(
            "Skipping fixture test - fixture not found at {:?}",
            fixture_path
        );
        return;
    }

    println!("Loading fixture repo from {:?}", fixture_path);

    // Import CAR into storage
    let storage = Arc::new(MemoryBlockStore::new());
    let header = read_car_header(fixture_path).await.unwrap();
    let parsed_car = read_car(fixture_path).await.unwrap();

    storage.put_many(parsed_car).await.unwrap();

    let root_cid = header.first().unwrap();

    // Load repository from fixture
    let mut repo = Repository::from_commit(storage.clone(), root_cid)
        .await
        .unwrap();

    println!(
        "Loaded fixture repo with commit at {}",
        repo.current_commit_cid()
    );

    let collection = Nsid::new("app.bsky.feed.post").unwrap();
    let signing_key = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
    let pubkey = get_public_key(&signing_key);
    let did = repo.did().clone().into_static();

    let mut rng = rand::thread_rng();
    let mut tracker = RecordTracker::new();

    // Perform random operations on fixture repo
    for batch_num in 1..=20 {
        let batch_size = rng.gen_range(10..=50);
        let ops = generate_random_ops(&mut rng, &mut tracker, batch_size);
        let record_writes = test_ops_to_record_writes(ops, &collection);

        let (repo_ops, commit_data) = repo
            .create_commit(
                &record_writes,
                &did,
                Some(repo.current_commit_cid().clone()),
                &signing_key,
            )
            .await
            .unwrap();

        repo.apply_commit(commit_data.clone()).await.unwrap();

        let firehose_commit = commit_data
            .to_firehose_commit(&did, batch_num, Datetime::now(), repo_ops, vec![])
            .await
            .unwrap();

        firehose_commit
            .validate_v1_1(&pubkey)
            .await
            .unwrap_or_else(|e| panic!("Fixture validation failed at batch {}: {}", batch_num, e));
    }

    println!("Fixture stress test complete - 20 batches validated");
}
