//! Proposal-0016 permissioned repository primitives.
//!
//! The wire and cryptographic choices in this module follow the checked-out
//! atproto `permissioned-data` implementation.  Ordinary repository commits,
//! MSTs, storage, and CAR APIs intentionally remain separate.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bytes::Bytes;
use cid::Cid;
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use jacquard_common::types::did::validate_did;
use jacquard_common::types::nsid::validate_nsid;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const LTHASH_BYTES: usize = 2048;
const LTHASH_LANES: usize = 1024;
const COMMIT_DOMAIN: &[u8] = b"atproto-space-v1";

/// Errors raised while validating permissioned protocol data.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PermissionedError {
    /// A string did not satisfy the protocol syntax or length bound.
    #[error("invalid {field}: {value}")]
    InvalidComponent { field: &'static str, value: String },
    /// A space declaration did not satisfy the required shape.
    #[error("invalid space declaration: {0}")]
    InvalidDeclaration(String),
    /// A write group violated a state-transition rule.
    #[error("invalid write group: {0}")]
    InvalidWrite(String),
    /// A signed commit failed validation.
    #[error("invalid signed commit: {0}")]
    InvalidCommit(String),
    /// A CAR failed ordered two-root validation.
    #[error("invalid permissioned CAR: {0}")]
    InvalidCar(String),
    /// A credential or proof failed validation.
    #[error("invalid credential: {0}")]
    InvalidCredential(String),
    /// The requested page starts before retained oplog history.
    #[error("oplog cursor is no longer available")]
    SinceUnavailable,
    /// A terminal page's advertised hash did not match the validated state.
    #[error("oplog terminal hash mismatch")]
    TerminalHashMismatch,
    /// A serialization operation failed.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Result alias for permissioned operations.
pub type Result<T> = std::result::Result<T, PermissionedError>;

fn valid_component(field: &'static str, value: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.len() > max || !value.is_char_boundary(value.len()) {
        return Err(PermissionedError::InvalidComponent {
            field,
            value: value.into(),
        });
    }
    Ok(())
}

fn valid_rkey(field: &'static str, value: &str) -> Result<()> {
    valid_component(field, value, 512)?;
    if value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_:~".contains(&b))
    {
        return Err(PermissionedError::InvalidComponent {
            field,
            value: value.into(),
        });
    }
    Ok(())
}

fn valid_nsid_component(field: &'static str, value: &str) -> Result<()> {
    valid_component(field, value, 317)?;
    validate_nsid(value).map_err(|_| PermissionedError::InvalidComponent {
        field,
        value: value.into(),
    })
}

fn valid_did_component(field: &'static str, value: &str) -> Result<()> {
    valid_component(field, value, 2048)?;
    validate_did(value).map_err(|_| PermissionedError::InvalidComponent {
        field,
        value: value.into(),
    })
}

/// A canonical permissioned space URI or record URI.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PermissionedPath(String);

impl PermissionedPath {
    /// Build a space URI after validating all of its components.
    pub fn space(space_did: &str, space_type: &str, skey: &str) -> Result<Self> {
        valid_did_component("space DID", space_did)?;
        valid_nsid_component("space type", space_type)?;
        valid_rkey("skey", skey)?;
        Ok(Self(format!("at://{space_did}/space/{space_type}/{skey}")))
    }

    /// Build a record URI after validating space ownership and path components.
    pub fn record(
        space_did: &str,
        space_type: &str,
        skey: &str,
        author_did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Self> {
        let space = Self::space(space_did, space_type, skey)?;
        valid_did_component("author DID", author_did)?;
        valid_nsid_component("collection", collection)?;
        valid_rkey("rkey", rkey)?;
        Ok(Self(format!(
            "{}/{author_did}/{collection}/{rkey}",
            space.0
        )))
    }

    /// Parse and validate a canonical permissioned URI.
    pub fn parse(uri: &str) -> Result<Self> {
        let parts: Vec<_> = uri.split('/').collect();
        if parts.len() != 6 && parts.len() != 9 || parts.first() != Some(&"at:") {
            return Err(PermissionedError::InvalidComponent {
                field: "permissioned URI",
                value: uri.into(),
            });
        }
        let space = Self::space(parts[2], parts[4], parts[5])?;
        if parts.len() == 6 {
            if space.0 != uri {
                return Err(PermissionedError::InvalidComponent {
                    field: "permissioned URI",
                    value: uri.into(),
                });
            }
            return Ok(space);
        }
        let record = Self::record(parts[2], parts[4], parts[5], parts[6], parts[7], parts[8])?;
        if record.0 != uri {
            return Err(PermissionedError::InvalidComponent {
                field: "permissioned URI",
                value: uri.into(),
            });
        }
        Ok(record)
    }

    /// Return the canonical URI string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this path names a record rather than a space.
    pub fn is_record(&self) -> bool {
        self.0.split('/').count() == 9
    }
}

impl fmt::Display for PermissionedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated `type: "space"` Lexicon declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceTypeDeclaration {
    /// Declaration NSID.
    pub nsid: String,
    /// Declared stable key.
    pub key: String,
    /// Human-readable name.
    pub name: String,
    /// Collections accepted by default when issuing OAuth grants.
    pub collections: Vec<String>,
    /// Optional description.
    pub description: Option<String>,
    /// Optional localized names.
    pub names: BTreeMap<String, String>,
}

impl SpaceTypeDeclaration {
    /// Parse a resolved Lexicon document, accepting only a `space` main def.
    pub fn from_lexicon(nsid: &str, document: &serde_json::Value) -> Result<Self> {
        valid_nsid_component("space type", nsid)?;
        let id = document.get("id").and_then(serde_json::Value::as_str);
        if id != Some(nsid) {
            return Err(PermissionedError::InvalidDeclaration(
                "document id does not match requested NSID".into(),
            ));
        }
        let main = document.get("defs").and_then(|v| v.get("main"));
        if main
            .and_then(|v| v.get("type"))
            .and_then(serde_json::Value::as_str)
            != Some("space")
        {
            return Err(PermissionedError::InvalidDeclaration(
                "defs.main is not type space".into(),
            ));
        }
        let key = main
            .and_then(|v| v.get("key"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| PermissionedError::InvalidDeclaration("missing string key".into()))?;
        valid_rkey("key", key)?;
        let name = main
            .and_then(|v| v.get("name"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| PermissionedError::InvalidDeclaration("missing name".into()))?;
        let graphemes = name.chars().count();
        if !(1..=64).contains(&graphemes) {
            return Err(PermissionedError::InvalidDeclaration(
                "name must contain 1..=64 characters".into(),
            ));
        }
        let values = main
            .and_then(|v| v.get("collections"))
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                PermissionedError::InvalidDeclaration("missing collections array".into())
            })?;
        let mut collections = Vec::with_capacity(values.len());
        for value in values {
            let collection = value.as_str().ok_or_else(|| {
                PermissionedError::InvalidDeclaration("collection is not a string".into())
            })?;
            valid_nsid_component("collection", collection)?;
            if collections.iter().any(|existing| existing == collection) {
                return Err(PermissionedError::InvalidDeclaration(
                    "duplicate collection".into(),
                ));
            }
            collections.push(collection.to_owned());
        }
        if collections.is_empty() {
            return Err(PermissionedError::InvalidDeclaration(
                "collections cannot be empty".into(),
            ));
        }
        let description = main
            .and_then(|v| v.get("description"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let names = main
            .and_then(|v| v.get("name:lang"))
            .and_then(serde_json::Value::as_object)
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            nsid: nsid.into(),
            key: key.into(),
            name: name.into(),
            collections,
            description,
            names,
        })
    }
}

/// Homomorphic set hash used by permissioned commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LtHash {
    state: [u8; LTHASH_BYTES],
}

impl Default for LtHash {
    fn default() -> Self {
        Self {
            state: [0; LTHASH_BYTES],
        }
    }
}

impl LtHash {
    /// Construct from a persisted 2048-byte state.
    pub fn from_state(state: &[u8]) -> Result<Self> {
        if state.len() != LTHASH_BYTES {
            return Err(PermissionedError::InvalidComponent {
                field: "LtHash state",
                value: state.len().to_string(),
            });
        }
        let mut value = [0; LTHASH_BYTES];
        value.copy_from_slice(state);
        Ok(Self { state: value })
    }
    /// Return a copy of the complete accumulator state.
    pub fn state(&self) -> [u8; LTHASH_BYTES] {
        self.state
    }
    /// Add an arbitrary UTF-8 element, modulo 2^16 per little-endian lane.
    pub fn add(&mut self, element: &str) {
        self.combine(element, true);
    }
    /// Remove an arbitrary UTF-8 element, modulo 2^16 per little-endian lane.
    pub fn remove(&mut self, element: &str) {
        self.combine(element, false);
    }
    /// Replace one element with another.
    pub fn update(&mut self, old: &str, new: &str) {
        self.remove(old);
        self.add(new);
    }
    /// Digest the full state with SHA-256.
    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.state).into()
    }
    /// Whether every lane is zero.
    pub fn is_empty(&self) -> bool {
        self.state.iter().all(|byte| *byte == 0)
    }
    fn combine(&mut self, element: &str, add: bool) {
        let mut expanded = [0u8; LTHASH_BYTES];
        let mut hasher = blake3::Hasher::new();
        hasher.update(element.as_bytes());
        hasher.finalize_xof().fill(&mut expanded);
        for i in 0..LTHASH_LANES {
            let offset = i * 2;
            let left = u16::from_le_bytes([self.state[offset], self.state[offset + 1]]);
            let right = u16::from_le_bytes([expanded[offset], expanded[offset + 1]]);
            let value = if add {
                left.wrapping_add(right)
            } else {
                left.wrapping_sub(right)
            };
            self.state[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
}

/// Inputs used to frame a signed permissioned commit context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitContext {
    pub space: String,
    pub author: String,
    pub rev: String,
}

/// Deniable permissioned commit. `sig` authenticates context only; `mac` binds hash to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCommit {
    /// Commit version, currently 1.
    pub ver: u8,
    /// SHA-256 digest of the LtHash state.
    pub hash: [u8; 32],
    /// Per-commit input keying material.
    pub ikm: [u8; 32],
    /// HMAC over hash with the context-derived key.
    pub mac: [u8; 32],
    /// Ed25519 signature over the framed context.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
    /// Host-assigned revision TID.
    pub rev: String,
}

impl SignedCommit {
    /// Sign a commit with fresh random IKM.
    pub fn sign(
        hash: [u8; 32],
        context: &CommitContext,
        key: &ed25519_dalek::SigningKey,
    ) -> Result<Self> {
        let mut ikm = [0; 32];
        rand::thread_rng().fill_bytes(&mut ikm);
        Self::sign_with_ikm(hash, context, key, ikm)
    }
    /// Sign a commit with explicit IKM, used for independent conformance vectors.
    pub fn sign_with_ikm(
        hash: [u8; 32],
        context: &CommitContext,
        key: &ed25519_dalek::SigningKey,
        ikm: [u8; 32],
    ) -> Result<Self> {
        let context_bytes = encode_commit_context(context, &ikm)?;
        let mac = compute_mac(&ikm, &context_bytes, &hash);
        Ok(Self {
            ver: 1,
            hash,
            ikm,
            mac,
            sig: key.sign(&context_bytes).to_bytes().to_vec(),
            rev: context.rev.clone(),
        })
    }
    /// Verify version, revision, MAC, and Ed25519 context signature.
    pub fn verify(&self, context: &CommitContext, key: &VerifyingKey) -> Result<()> {
        if self.ver != 1 || self.rev != context.rev {
            return Err(PermissionedError::InvalidCommit(
                "version or revision mismatch".into(),
            ));
        }
        let context_bytes = encode_commit_context(context, &self.ikm)?;
        let expected = compute_mac(&self.ikm, &context_bytes, &self.hash);
        if expected != self.mac {
            return Err(PermissionedError::InvalidCommit("MAC mismatch".into()));
        }
        let signature = Signature::from_slice(&self.sig)
            .map_err(|_| PermissionedError::InvalidCommit("invalid signature bytes".into()))?;
        key.verify(&context_bytes, &signature)
            .map_err(|_| PermissionedError::InvalidCommit("signature mismatch".into()))
    }
    /// Encode the commit as canonical DAG-CBOR.
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        serde_ipld_dagcbor::to_vec(self)
            .map_err(|error| PermissionedError::Serialization(error.to_string()))
    }
    /// Decode a DAG-CBOR commit.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self> {
        serde_ipld_dagcbor::from_slice(bytes)
            .map_err(|error| PermissionedError::Serialization(error.to_string()))
    }
}

fn encode_commit_context(context: &CommitContext, ikm: &[u8; 32]) -> Result<Vec<u8>> {
    let fields = [
        context.space.as_bytes(),
        context.author.as_bytes(),
        context.rev.as_bytes(),
        ikm.as_slice(),
    ];
    let mut output = COMMIT_DOMAIN.to_vec();
    for field in fields {
        if field.len() > u16::MAX as usize {
            return Err(PermissionedError::InvalidCommit(
                "context field too long".into(),
            ));
        }
        output.extend_from_slice(&(field.len() as u16).to_be_bytes());
        output.extend_from_slice(field);
    }
    Ok(output)
}

fn compute_mac(ikm: &[u8; 32], context: &[u8], hash: &[u8; 32]) -> [u8; 32] {
    let hkdf = hkdf::Hkdf::<Sha256>::from_prk(ikm).expect("32-byte PRK is valid");
    let mut key = [0; 32];
    hkdf.expand(context, &mut key)
        .expect("fixed output length is valid");
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("HMAC accepts any key");
    mac.update(hash);
    mac.finalize().into_bytes().into()
}

/// A pure record write candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOperation {
    Create {
        uri: String,
        cid: String,
        value: Bytes,
    },
    Update {
        uri: String,
        prev: String,
        cid: String,
        value: Bytes,
    },
    Delete {
        uri: String,
        prev: String,
    },
}

/// Current in-memory values used by pure write validation and conformance tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteState {
    pub records: BTreeMap<String, RecordValue>,
    pub lthash: LtHash,
}

/// A current record value and CID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordValue {
    pub cid: String,
    pub value: Bytes,
    pub author: String,
}

/// One write's result in input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteResult {
    pub uri: String,
    pub cid: Option<String>,
}

/// Oplog action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OplogAction {
    Create,
    Update,
    Delete,
}

/// Reference-shaped oplog entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OplogEntry {
    pub space: String,
    pub rev: String,
    pub idx: u32,
    pub action: OplogAction,
    pub uri: String,
    pub collection: String,
    pub rkey: String,
    pub cid: Option<String>,
    pub prev: Option<String>,
}

/// Atomic result of validating and applying a write group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWritesResult {
    pub revision: String,
    pub results: Vec<WriteResult>,
    pub oplog: Vec<OplogEntry>,
    pub lthash: LtHash,
}

/// Validate and apply a complete group to a cloned state, swapping only on success.
pub fn apply_writes(
    state: &mut WriteState,
    space: &str,
    revision: &str,
    operations: &[WriteOperation],
) -> Result<ApplyWritesResult> {
    if operations.len() > 200 {
        return Err(PermissionedError::InvalidWrite("maximum 200 writes".into()));
    }
    valid_component("revision", revision, 13)?;
    if jacquard_common::types::tid::Tid::new(revision).is_err() {
        return Err(PermissionedError::InvalidWrite(
            "revision must be a TID".into(),
        ));
    }
    let mut candidate = state.clone();
    let mut results = Vec::with_capacity(operations.len());
    let mut oplog = Vec::with_capacity(operations.len());
    for (idx, operation) in operations.iter().enumerate() {
        let (uri, action, cid, prev, value, author) = match operation {
            WriteOperation::Create { uri, cid, value } => (
                uri,
                OplogAction::Create,
                Some(cid.clone()),
                None,
                Some(value.clone()),
                uri.split('/').nth(6).unwrap_or_default().to_owned(),
            ),
            WriteOperation::Update {
                uri,
                prev,
                cid,
                value,
            } => (
                uri,
                OplogAction::Update,
                Some(cid.clone()),
                Some(prev.clone()),
                Some(value.clone()),
                uri.split('/').nth(6).unwrap_or_default().to_owned(),
            ),
            WriteOperation::Delete { uri, prev } => (
                uri,
                OplogAction::Delete,
                None,
                Some(prev.clone()),
                None,
                uri.split('/').nth(6).unwrap_or_default().to_owned(),
            ),
        };
        let path = PermissionedPath::parse(uri)?;
        if !path.is_record() {
            return Err(PermissionedError::InvalidWrite(
                "write URI must name a record".into(),
            ));
        }
        let key = uri.to_owned();
        let existing = candidate.records.get(&key).cloned();
        match operation {
            WriteOperation::Create { .. } if existing.is_some() => {
                return Err(PermissionedError::InvalidWrite(
                    "record already exists".into(),
                ));
            }
            WriteOperation::Update { prev, .. } | WriteOperation::Delete { prev, .. } => {
                let record = existing.ok_or_else(|| {
                    PermissionedError::InvalidWrite("record does not exist".into())
                })?;
                if record.cid != *prev {
                    return Err(PermissionedError::InvalidWrite("prev CID mismatch".into()));
                }
                candidate.lthash.remove(&format!(
                    "{}/{}/{}",
                    path_collection(uri)?,
                    path_rkey(uri)?,
                    record.cid
                ));
            }
            WriteOperation::Create { .. } => {}
        }
        if let Some(cid_value) = &cid {
            if let Some(record_value) = &value {
                candidate.lthash.add(&format!(
                    "{}/{}/{}",
                    path_collection(uri)?,
                    path_rkey(uri)?,
                    cid_value
                ));
                candidate.records.insert(
                    key.clone(),
                    RecordValue {
                        cid: cid_value.clone(),
                        value: record_value.clone(),
                        author,
                    },
                );
            } else {
                candidate.records.remove(&key);
            }
        }
        let (collection, rkey) = (path_collection(uri)?, path_rkey(uri)?);
        oplog.push(OplogEntry {
            space: space.into(),
            rev: revision.into(),
            idx: idx as u32,
            action,
            uri: uri.into(),
            collection,
            rkey,
            cid,
            prev,
        });
        results.push(WriteResult {
            uri: key,
            cid: match operation {
                WriteOperation::Delete { .. } => None,
                _ => oplog.last().and_then(|entry| entry.cid.clone()),
            },
        });
    }
    let result = ApplyWritesResult {
        revision: revision.into(),
        results,
        oplog,
        lthash: candidate.lthash.clone(),
    };
    *state = candidate;
    Ok(result)
}

fn path_collection(uri: &str) -> Result<String> {
    uri.split('/')
        .nth(7)
        .map(str::to_owned)
        .ok_or_else(|| PermissionedError::InvalidWrite("missing collection".into()))
}
fn path_rkey(uri: &str) -> Result<String> {
    uri.split('/')
        .nth(8)
        .map(str::to_owned)
        .ok_or_else(|| PermissionedError::InvalidWrite("missing rkey".into()))
}

/// Parse the normative slash-separated cursor.
pub fn parse_cursor(cursor: &str) -> Result<(String, u32)> {
    let (rev, idx) = cursor
        .split_once('/')
        .ok_or_else(|| PermissionedError::InvalidWrite("cursor must use rev/idx".into()))?;
    if cursor.matches('/').count() != 1 || jacquard_common::types::tid::Tid::new(rev).is_err() {
        return Err(PermissionedError::InvalidWrite("invalid cursor".into()));
    }
    let idx = idx
        .parse::<u32>()
        .map_err(|_| PermissionedError::InvalidWrite("invalid cursor index".into()))?;
    Ok((rev.into(), idx))
}

/// Format a cursor using the normative slash separator.
pub fn format_cursor(rev: &str, idx: u32) -> String {
    format!("{rev}/{idx}")
}

/// A page of ordered operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OplogPage {
    pub ops: Vec<OplogEntry>,
    pub cursor: Option<String>,
    pub commit: Option<SignedCommit>,
}

/// Page oplog rows in revision/index order. A commit is included only on a short terminal page.
pub fn list_repo_ops(
    entries: &[OplogEntry],
    since: Option<&str>,
    cursor: Option<&str>,
    limit: Option<usize>,
    commit: Option<SignedCommit>,
) -> Result<OplogPage> {
    let limit = limit.unwrap_or(100).min(1000);
    let start = if let Some(cursor) = cursor {
        parse_cursor(cursor)?
    } else if let Some(since) = since {
        (
            jacquard_common::types::tid::Tid::new(since)
                .map_err(|_| PermissionedError::SinceUnavailable)?
                .as_str()
                .into(),
            u32::MAX,
        )
    } else {
        (String::new(), u32::MAX)
    };
    let mut ordered = entries.to_vec();
    ordered.sort_by(|left, right| left.rev.cmp(&right.rev).then(left.idx.cmp(&right.idx)));
    if since.is_some()
        && ordered.first().is_some_and(|entry| entry.rev > start.0)
        && !ordered.iter().any(|entry| entry.rev == start.0)
    {
        return Err(PermissionedError::SinceUnavailable);
    }
    let filtered: Vec<_> = ordered
        .into_iter()
        .filter(|entry| (entry.rev > start.0) || (entry.rev == start.0 && entry.idx > start.1))
        .collect();
    let terminal = filtered.len() <= limit;
    let ops = filtered.into_iter().take(limit).collect::<Vec<_>>();
    let next = if terminal {
        None
    } else {
        ops.last().map(|entry| format_cursor(&entry.rev, entry.idx))
    };
    Ok(OplogPage {
        ops,
        cursor: next,
        commit: terminal.then_some(commit).flatten(),
    })
}

/// Credential class used by host exchanges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialKind {
    Delegation,
    Space,
    ClientAttestation,
}

/// Claims independent of a JWT library; host code supplies signing and key resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialClaims {
    pub typ: CredentialKind,
    pub iss: String,
    pub sub: String,
    pub aud: Option<String>,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
    pub cnf_jkt: Option<String>,
}

impl CredentialClaims {
    /// Validate issuer, subject, audience, expiry, and DPoP binding semantics.
    pub fn validate(
        &self,
        now: i64,
        expected_subject: &str,
        expected_audience: Option<&str>,
        required_jkt: Option<&str>,
    ) -> Result<()> {
        if self.sub != expected_subject || self.exp < now || self.iat > now + 5 {
            return Err(PermissionedError::InvalidCredential(
                "subject or time claim mismatch".into(),
            ));
        }
        if self.aud.as_deref() != expected_audience {
            return Err(PermissionedError::InvalidCredential(
                "audience mismatch".into(),
            ));
        }
        if self.cnf_jkt.as_deref() != required_jkt {
            return Err(PermissionedError::InvalidCredential(
                "DPoP thumbprint mismatch".into(),
            ));
        }
        Ok(())
    }
}

/// DPoP proof claims after JWT signature verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpopProof {
    pub jti: String,
    pub htm: String,
    pub htu: String,
    pub ath: String,
    pub iat: i64,
}

/// Validate DPoP method, URL, credential hash, age, and replay through a caller-owned set.
pub fn verify_dpop(
    proof: &DpopProof,
    method: &str,
    url: &str,
    credential: &[u8],
    now: i64,
    replay: &mut BTreeSet<String>,
) -> Result<()> {
    if proof.htm != method
        || normalize_htu(&proof.htu) != normalize_htu(url)
        || proof.iat < now - 65
        || proof.iat > now + 5
    {
        return Err(PermissionedError::InvalidCredential(
            "DPoP method, URL, or age mismatch".into(),
        ));
    }
    let digest = Sha256::digest(credential);
    let expected = base64url(&digest);
    if proof.ath != expected {
        return Err(PermissionedError::InvalidCredential(
            "DPoP ath mismatch".into(),
        ));
    }
    if !replay.insert(proof.jti.clone()) {
        return Err(PermissionedError::InvalidCredential("DPoP replay".into()));
    }
    Ok(())
}

/// Remove query and fragment from a DPoP URL.
pub fn normalize_htu(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_owned()
}
fn base64url(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let a = bytes[i];
        let b = bytes.get(i + 1).copied().unwrap_or(0);
        let c = bytes.get(i + 2).copied().unwrap_or(0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[((a & 3) << 4 | b >> 4) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(TABLE[((b & 15) << 2 | c >> 6) as usize] as char);
        }
        if i + 2 < bytes.len() {
            out.push(TABLE[(c & 63) as usize] as char);
        }
        i += 3;
    }
    out
}

/// A permissioned-space OAuth resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpacePermission {
    pub space_type: String,
    pub authority: Option<String>,
    pub skey: Option<String>,
    pub collection: Vec<String>,
    pub action: Vec<String>,
    pub manage: Vec<String>,
}

impl SpacePermission {
    /// Validate a permission-set entry; wildcard space types are forbidden in sets.
    pub fn validate(&self, namespace: &str) -> Result<()> {
        valid_nsid_component("space type", &self.space_type)?;
        if self.space_type == "*" {
            return Err(PermissionedError::InvalidCredential(
                "permission sets require a concrete space type".into(),
            ));
        }
        for collection in &self.collection {
            if collection != "*" {
                valid_nsid_component("collection", collection)?;
            }
        }
        if let Some(authority) = &self.authority {
            if authority != "self" && authority != "*" {
                valid_did_component("authority", authority)?;
            }
        }
        if !self.space_type.starts_with(namespace) {
            return Err(PermissionedError::InvalidCredential(
                "permission outside namespace".into(),
            ));
        }
        Ok(())
    }
    /// Match independently against a requested space and action.
    pub fn matches(&self, space_type: &str, collection: &str, action: &str) -> bool {
        (self.space_type == space_type || self.space_type == "*")
            && (self.collection.is_empty()
                || self
                    .collection
                    .iter()
                    .any(|value| value == "*" || value == collection))
            && (self.action.is_empty()
                || self
                    .action
                    .iter()
                    .any(|value| value == action || (action == "read_self" && value == "read")))
    }
}

/// Ordered two-root permissioned CAR representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionedCar {
    /// Signed commit root first.
    pub roots: [Cid; 2],
    /// Commit, index, then records in canonical order.
    pub blocks: Vec<(Cid, Bytes)>,
}

/// Immutable result returned after CAR verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRepoSnapshot {
    /// Verified roots.
    pub roots: [Cid; 2],
    /// Verified blocks in input order.
    pub blocks: Vec<(Cid, Bytes)>,
}

impl PermissionedCar {
    /// Construct and validate a two-root CAR representation.
    pub fn new(roots: [Cid; 2], blocks: Vec<(Cid, Bytes)>) -> Result<Self> {
        let car = Self { roots, blocks };
        car.validate()?;
        Ok(car)
    }
    /// Return a roots-only CAR representation.
    pub fn exclude_values(&self) -> Self {
        Self {
            roots: self.roots,
            blocks: self.blocks.iter().take(2).cloned().collect(),
        }
    }
    /// Verify root count/order, block CIDs, and duplicate blocks.
    pub fn validate(&self) -> Result<()> {
        if self.blocks.len() < 2
            || self.blocks[0].0 != self.roots[0]
            || self.blocks[1].0 != self.roots[1]
        {
            return Err(PermissionedError::InvalidCar(
                "commit/index roots must lead the block stream".into(),
            ));
        }
        let mut seen = BTreeSet::new();
        for (cid, bytes) in &self.blocks {
            if !seen.insert(*cid) {
                return Err(PermissionedError::InvalidCar("duplicate block".into()));
            }
            let actual = crate::mst::util::compute_cid(bytes)
                .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
            if actual != *cid {
                return Err(PermissionedError::InvalidCar("block CID mismatch".into()));
            }
        }
        Ok(())
    }
    /// Consume the CAR into an immutable validated snapshot.
    pub fn snapshot(self) -> Result<ValidatedRepoSnapshot> {
        self.validate()?;
        Ok(ValidatedRepoSnapshot {
            roots: self.roots,
            blocks: self.blocks,
        })
    }
}

impl ValidatedRepoSnapshot {
    /// Return the ordered roots.
    pub fn roots(&self) -> &[Cid; 2] {
        &self.roots
    }

    /// Return verified blocks without exposing mutable state.
    pub fn blocks(&self) -> &[(Cid, Bytes)] {
        &self.blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn lthash_empty_reference_vector() {
        assert_eq!(
            hex::encode(LtHash::default().digest()),
            "e5a00aa9991ac8a5ee3109844d84a55583bd20572ad3ffcd42792f3c36b183ad"
        );
    }
    #[test]
    fn lthash_two_element_reference_vector() {
        let mut hash = LtHash::default();
        hash.add("one");
        hash.add("two");
        assert_eq!(
            hex::encode(hash.digest()),
            "ae05cb6d224379d9710c290c8529945c5b0e0fde9ead30b9699057ce701c63e7"
        );
    }
    #[test]
    fn lthash_order_and_inverse() {
        let mut left = LtHash::default();
        left.add("one");
        left.add("two");
        let mut right = LtHash::default();
        right.add("two");
        right.add("one");
        assert_eq!(left, right);
        right.remove("one");
        right.remove("two");
        assert!(right.is_empty());
    }
    #[test]
    fn signed_commit_context_and_round_trip() {
        let context = CommitContext {
            space: "at://did:plc:space/space/com.example.type/demo".into(),
            author: "did:plc:author".into(),
            rev: "3jzfcijpj2m2a".into(),
        };
        let key = SigningKey::from_bytes(&[7; 32]);
        let commit = SignedCommit::sign_with_ikm([9; 32], &context, &key, [0x20; 32]).unwrap();
        commit.verify(&context, &key.verifying_key()).unwrap();
        let bytes = commit.to_cbor().unwrap();
        assert_eq!(SignedCommit::from_cbor(&bytes).unwrap(), commit);
    }
    #[test]
    fn signed_commit_mutation_fails() {
        let context = CommitContext {
            space: "at://did:plc:space/space/com.example.type/demo".into(),
            author: "did:plc:author".into(),
            rev: "3jzfcijpj2m2a".into(),
        };
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut commit = SignedCommit::sign_with_ikm([9; 32], &context, &key, [0x20; 32]).unwrap();
        commit.hash[0] ^= 1;
        assert!(commit.verify(&context, &key.verifying_key()).is_err());
    }
    #[test]
    fn write_group_is_atomic_and_cursor_uses_slash() {
        let uri = PermissionedPath::record(
            "did:plc:space",
            "com.example.type",
            "demo",
            "did:plc:author",
            "com.example.record",
            "r",
        )
        .unwrap()
        .to_string();
        let mut state = WriteState::default();
        let ops = [WriteOperation::Create {
            uri: uri.clone(),
            cid: "bafybeigdyrzt5o5p4s5x6f7g8h9j0k1l2m3n4o5p6q7r8s9t0u".into(),
            value: Bytes::from_static(b"{}"),
        }];
        let result = apply_writes(
            &mut state,
            "at://did:plc:space/space/com.example.type/demo",
            "3jzfcijpj2m2a",
            &ops,
        )
        .unwrap();
        assert_eq!(result.oplog[0].idx, 0);
        assert_eq!(format_cursor("3jzfcijpj2m2a", 0), "3jzfcijpj2m2a/0");
    }
}
