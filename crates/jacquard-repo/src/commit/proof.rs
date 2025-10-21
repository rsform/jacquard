//! Record proof verification
//!
//! Verifies merkle proofs for individual record existence/non-existence.
//!
//! **Proof structure:**
//! - CAR file containing:
//!   - Commit block (with signature)
//!   - MST node blocks along the path to the record(s)
//!   - Record blocks (if proving existence)
//!
//! **Verification:**
//! 1. Parse CAR blocks into temporary storage
//! 2. Load and verify commit (signature + DID)
//! 3. Load MST using ONLY blocks from CAR
//! 4. For each claim, check if record exists/matches in MST
//!
//! This is distinct from firehose commit validation - proofs verify individual
//! records, not full repository commits.

use crate::BlockStore;
use crate::error::ProofError;
use crate::mst::Mst;
use crate::storage::MemoryBlockStore;
use cid::Cid as IpldCid;
use jacquard_common::types::string::Did;
use smol_str::format_smolstr;
use std::sync::Arc;

/// A claim about a record's CID at a specific path
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordClaim<'a> {
    /// Collection NSID (e.g., "app.bsky.feed.post")
    pub collection: jacquard_common::CowStr<'a>,

    /// Record key (TID or other identifier)
    pub rkey: jacquard_common::CowStr<'a>,

    /// Expected CID of the record
    /// - Some(cid): claiming record exists with this CID
    /// - None: claiming record does not exist
    pub cid: Option<IpldCid>,
}

/// Result of proof verification
#[derive(Debug)]
pub struct VerifyProofsOutput<'a> {
    /// Claims that were successfully verified
    pub verified: Vec<RecordClaim<'a>>,

    /// Claims that failed verification
    pub unverified: Vec<RecordClaim<'a>>,
}

/// Verify record proofs from a CAR file
///
/// **Inputs:**
/// - `car_bytes`: CAR file containing commit + MST blocks + record blocks
/// - `claims`: Records to verify (existence or non-existence)
/// - `did`: Expected DID of the repository
/// - `pubkey`: Public key for signature verification
///
/// **Returns:**
/// - `verified`: Claims that match the MST state
/// - `unverified`: Claims that don't match
///
/// **Security:**
/// - Verifies commit signature using provided pubkey
/// - Verifies DID matches
/// - Uses ONLY blocks from CAR (merkle proof property)
///
/// # Example
///
/// ```rust,ignore
/// let claims = vec![
///     RecordClaim {
///         collection: "app.bsky.feed.post".into(),
///         rkey: "3l4qpz7ajrc2a".into(),
///         cid: Some(record_cid), // Claiming this record exists
///     },
///     RecordClaim {
///         collection: "app.bsky.feed.post".into(),
///         rkey: "nonexistent".into(),
///         cid: None, // Claiming this record doesn't exist
///     },
/// ];
///
/// let result = verify_proofs(car_bytes, claims, did, pubkey).await?;
/// assert_eq!(result.verified.len(), 2); // Both claims verified
/// ```
pub async fn verify_proofs<'a>(
    car_bytes: &[u8],
    claims: Vec<RecordClaim<'a>>,
    did: &Did<'_>,
    pubkey: &jacquard_common::types::crypto::PublicKey<'_>,
) -> Result<VerifyProofsOutput<'a>, ProofError> {
    // 1. Parse CAR file
    let parsed =
        crate::car::parse_car_bytes(car_bytes)
            .await
            .map_err(|e| ProofError::CarParseFailed {
                source: Box::new(e),
            })?;

    // 2. Create storage with ONLY blocks from CAR (merkle proof property)
    let storage = Arc::new(MemoryBlockStore::new_from_blocks(parsed.blocks));

    // 3. Load commit from CAR root
    let commit_cid = parsed.root;
    let commit_bytes = storage
        .get(&commit_cid)
        .await
        .map_err(|_| ProofError::CommitNotFound)?
        .ok_or(ProofError::CommitNotFound)?;

    let commit = super::Commit::from_cbor(&commit_bytes).map_err(|e| {
        ProofError::CommitDeserializeFailed {
            source: Box::new(e),
        }
    })?;

    // 4. Verify DID matches
    if commit.did().as_ref() != did.as_ref() {
        return Err(ProofError::DidMismatch {
            commit_did: commit.did().to_string(),
            expected_did: did.to_string(),
        }
        .into());
    }

    // 5. Verify signature
    // We need to extract the CommitError before it gets converted to RepoError
    if let Err(e) = commit.verify(pubkey) {
        return Err(ProofError::SignatureVerificationFailed { source: e }.into());
    }

    // 6. Load MST using ONLY blocks from CAR
    let mst = Mst::load(storage.clone(), *commit.data(), None);

    // 7. Verify each claim
    let mut verified = Vec::new();
    let mut unverified = Vec::new();

    for claim in claims {
        let key = format_smolstr!("{}/{}", claim.collection, claim.rkey);
        let found_cid = mst.get(&key).await.ok().flatten();

        match (&claim.cid, found_cid) {
            // Claiming record doesn't exist
            (None, None) => {
                // Correct: record doesn't exist
                verified.push(claim);
            }
            (None, Some(_)) => {
                // Incorrect: claimed doesn't exist but it does
                unverified.push(claim);
            }
            // Claiming record exists with specific CID
            (Some(claimed_cid), Some(found)) if claimed_cid == &found => {
                // Correct: CID matches
                verified.push(claim);
            }
            (Some(_), _) => {
                // Incorrect: CID mismatch or doesn't exist
                unverified.push(claim);
            }
        }
    }

    Ok(VerifyProofsOutput {
        verified,
        unverified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::Commit;
    use crate::mst::Mst;
    use crate::storage::MemoryBlockStore;
    use jacquard_common::types::crypto::PublicKey;
    use jacquard_common::types::string::Did;

    fn test_signing_key() -> k256::ecdsa::SigningKey {
        use k256::ecdsa::SigningKey;
        use rand::rngs::OsRng;
        SigningKey::random(&mut OsRng)
    }

    fn test_pubkey(sk: &k256::ecdsa::SigningKey) -> PublicKey<'static> {
        use jacquard_common::types::crypto::KeyCodec;
        use std::borrow::Cow;
        let vk = sk.verifying_key();
        PublicKey {
            codec: KeyCodec::Secp256k1,
            bytes: Cow::Owned(vk.to_encoded_point(true).as_bytes().to_vec()),
        }
    }

    fn test_cid(n: u8) -> IpldCid {
        let data = vec![n; 32];
        let mh =
            multihash::Multihash::wrap(jacquard_common::types::crypto::SHA2_256, &data).unwrap();
        IpldCid::new_v1(crate::DAG_CBOR_CID_CODEC, mh)
    }

    #[tokio::test]
    async fn test_verify_proofs_record_exists() {
        // Create MST with records
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let cid1 = test_cid(1);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        // Create and sign commit
        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        // Generate CAR proof with: commit + MST path blocks
        let cids_for_proof = mst.cids_for_path(key1).await.unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();

        // Add commit block
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        // Add MST blocks
        for cid in &cids_for_proof[..cids_for_proof.len() - 1] {
            // All except record CID
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        // Add record block
        car_blocks.insert(cid1, bytes::Bytes::from(vec![0x42])); // dummy record data

        // Write CAR
        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        // Verify proof
        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: Some(cid1),
        }];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert_eq!(result.verified.len(), 1);
        assert_eq!(result.unverified.len(), 0);
    }

    #[tokio::test]
    async fn test_verify_proofs_record_not_exists() {
        // Create MST with one record
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let cid1 = test_cid(1);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        // Create and sign commit
        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        // Generate proof for non-existent record
        let nonexistent_key = "app.bsky.feed.post/xyz789";
        let cids_for_proof = mst.cids_for_path(nonexistent_key).await.unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();

        // Add commit block
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        // Add MST blocks (proof of absence)
        for cid in &cids_for_proof {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        // Write CAR
        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        // Verify proof of non-existence
        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "xyz789".into(),
            cid: None, // Claiming it doesn't exist
        }];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert_eq!(result.verified.len(), 1);
        assert_eq!(result.unverified.len(), 0);
    }

    #[tokio::test]
    async fn test_verify_proofs_multiple_claims_mixed() {
        // Test verifying multiple claims - some valid, some invalid
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let key2 = "app.bsky.feed.post/def456";
        let cid1 = test_cid(1);
        let cid2 = test_cid(2);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst = mst.add(key2, cid2).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        // Generate CAR with both records
        let cids_for_proof1 = mst.cids_for_path(key1).await.unwrap();
        let cids_for_proof2 = mst.cids_for_path(key2).await.unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();

        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        // Add all MST blocks from both paths
        for cid in cids_for_proof1
            .iter()
            .chain(cids_for_proof2.iter())
            .take(cids_for_proof1.len() + cids_for_proof2.len() - 2)
        {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        // Add record blocks
        car_blocks.insert(cid1, bytes::Bytes::from(vec![0x41]));
        car_blocks.insert(cid2, bytes::Bytes::from(vec![0x42]));

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        // Mixed claims: valid, invalid CID, non-existent
        let claims = vec![
            RecordClaim {
                collection: "app.bsky.feed.post".into(),
                rkey: "abc123".into(),
                cid: Some(cid1), // Valid
            },
            RecordClaim {
                collection: "app.bsky.feed.post".into(),
                rkey: "def456".into(),
                cid: Some(test_cid(99)), // Wrong CID
            },
            RecordClaim {
                collection: "app.bsky.feed.post".into(),
                rkey: "xyz789".into(),
                cid: None, // Correctly doesn't exist
            },
        ];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert_eq!(result.verified.len(), 2); // First and third should verify
        assert_eq!(result.unverified.len(), 1); // Second should fail
    }

    #[tokio::test]
    async fn test_verify_proofs_wrong_did() {
        // Test that verification fails when DID doesn't match
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let cid1 = test_cid(1);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        let cids_for_proof = mst.cids_for_path(key1).await.unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        for cid in &cids_for_proof[..cids_for_proof.len() - 1] {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        car_blocks.insert(cid1, bytes::Bytes::from(vec![0x42]));

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: Some(cid1),
        }];

        // Try to verify with WRONG DID
        let wrong_did = Did::new("did:plc:wrong").unwrap();
        let result = verify_proofs(&car_bytes, claims, &wrong_did, &pubkey).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("DID mismatch"));
    }

    #[tokio::test]
    async fn test_verify_proofs_bad_signature() {
        // Test that verification fails with wrong public key
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let cid1 = test_cid(1);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        let sk = test_signing_key();
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        let cids_for_proof = mst.cids_for_path(key1).await.unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        for cid in &cids_for_proof[..cids_for_proof.len() - 1] {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        car_blocks.insert(cid1, bytes::Bytes::from(vec![0x42]));

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: Some(cid1),
        }];

        // Use WRONG public key
        let wrong_sk = test_signing_key();
        let wrong_pubkey = test_pubkey(&wrong_sk);

        let result = verify_proofs(&car_bytes, claims, &did, &wrong_pubkey).await;

        // Should fail signature verification
        assert!(matches!(
            result,
            Err(ProofError::SignatureVerificationFailed { source: _ })
        ));
    }

    #[tokio::test]
    async fn test_verify_proofs_missing_blocks() {
        // Test that verification fails when CAR is missing necessary blocks
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let cid1 = test_cid(1);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        // Create CAR with ONLY commit block, missing MST blocks
        let mut car_blocks = std::collections::BTreeMap::new();
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));
        // Intentionally NOT adding MST blocks or record blocks

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: Some(cid1),
        }];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert!(result.verified.is_empty())
    }

    #[tokio::test]
    async fn test_verify_proofs_empty_mst() {
        // Test proof verification on empty MST (claiming non-existence)
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let mst_root = mst.persist().await.unwrap();

        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        let cids_for_proof = mst
            .cids_for_path("app.bsky.feed.post/abc123")
            .await
            .unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        // Add any MST blocks (empty MST might still have root node)
        for cid in &cids_for_proof {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: None, // Claiming doesn't exist
        }];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert_eq!(result.verified.len(), 1);
        assert_eq!(result.unverified.len(), 0);
    }

    #[tokio::test]
    async fn test_verify_proofs_claim_exists_in_empty_mst() {
        // Test that claiming existence in empty MST fails
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let mst_root = mst.persist().await.unwrap();

        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        let cids_for_proof = mst
            .cids_for_path("app.bsky.feed.post/abc123")
            .await
            .unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();
        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        for cid in &cids_for_proof {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: Some(test_cid(1)), // Claiming it exists
        }];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert_eq!(result.verified.len(), 0);
        assert_eq!(result.unverified.len(), 1); // Should fail
    }

    #[tokio::test]
    async fn test_verify_proofs_invalid_claim() {
        // Create MST with records
        let storage = Arc::new(MemoryBlockStore::new());
        let mst = Mst::new(storage.clone());

        let key1 = "app.bsky.feed.post/abc123";
        let cid1 = test_cid(1);

        let mst = mst.add(key1, cid1).await.unwrap();
        let mst_root = mst.persist().await.unwrap();

        // Create and sign commit
        let sk = test_signing_key();
        let pubkey = test_pubkey(&sk);
        let did = Did::new("did:plc:test").unwrap();

        let commit = Commit::new_unsigned(
            did.clone(),
            mst_root,
            jacquard_common::types::tid::Ticker::new().next(None),
            None,
        )
        .sign(&sk)
        .unwrap();

        let commit_cid = commit.to_cid().unwrap();
        let commit_bytes = commit.to_cbor().unwrap();
        storage.put(&commit_bytes).await.unwrap();

        // Generate CAR proof
        let cids_for_proof = mst.cids_for_path(key1).await.unwrap();
        let mut car_blocks = std::collections::BTreeMap::new();

        car_blocks.insert(commit_cid, bytes::Bytes::from(commit_bytes));

        for cid in &cids_for_proof[..cids_for_proof.len() - 1] {
            if let Some(block) = storage.get(cid).await.unwrap() {
                car_blocks.insert(*cid, block);
            }
        }

        car_blocks.insert(cid1, bytes::Bytes::from(vec![0x42]));

        let car_bytes = crate::car::write_car_bytes(commit_cid, car_blocks)
            .await
            .unwrap();

        // Verify proof with WRONG CID
        let wrong_cid = test_cid(99);
        let claims = vec![RecordClaim {
            collection: "app.bsky.feed.post".into(),
            rkey: "abc123".into(),
            cid: Some(wrong_cid), // Wrong CID
        }];

        let result = verify_proofs(&car_bytes, claims, &did, &pubkey)
            .await
            .unwrap();

        assert_eq!(result.verified.len(), 0);
        assert_eq!(result.unverified.len(), 1); // Failed verification
    }
}
