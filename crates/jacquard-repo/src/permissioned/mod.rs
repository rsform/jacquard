//! Proposal-0016 permissioned repository primitives.
//!
//! The wire and cryptographic choices in this module follow the checked-out
//! atproto `permissioned-data` implementation.  Ordinary repository commits,
//! MSTs, storage, and CAR APIs intentionally remain separate.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::str::FromStr;

use bytes::Bytes;
use cid::Cid as IpldCid;

type CarCid = IpldCid;
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use hmac::{Hmac, KeyInit, Mac};
use jacquard_common::SmolStr;
use jacquard_common::types::aturi::AtSpaceUri;
use jacquard_common::types::cid::{Cid as AtCid, CidLink};
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::Rkey;
use jacquard_common::types::tid::Tid;
use jacquard_common::types::value::Data;

type SpaceUri = AtSpaceUri<SmolStr>;
type DidOwned = Did<SmolStr>;
type Identifier = AtIdentifier<SmolStr>;
type NsidOwned = Nsid<SmolStr>;
type RkeyOwned = Rkey<SmolStr>;
type CidOwned = AtCid<SmolStr>;
use jacquard_lexicon::lexicon::{LexSpace, LexUserType, LexiconDoc};
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
    InvalidComponent {
        /// Name of the invalid component.
        field: &'static str,
        /// Supplied invalid value.
        value: String,
    },
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

fn invalid_component(field: &'static str, value: impl Into<String>) -> PermissionedError {
    PermissionedError::InvalidComponent {
        field,
        value: value.into(),
    }
}

fn parse_did(field: &'static str, value: impl AsRef<str>) -> Result<DidOwned> {
    DidOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_nsid(field: &'static str, value: impl AsRef<str>) -> Result<NsidOwned> {
    NsidOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_rkey(field: &'static str, value: impl AsRef<str>) -> Result<RkeyOwned> {
    RkeyOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_tid(field: &'static str, value: impl AsRef<str>) -> Result<Tid> {
    Tid::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

/// A validated `type: "space"` Lexicon declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceTypeDeclaration {
    /// Declaration NSID.
    pub nsid: NsidOwned,
    /// Declared stable key.
    pub key: RkeyOwned,
    /// Human-readable name.
    pub name: String,
    /// Collections accepted by default when issuing OAuth grants.
    pub collections: Vec<NsidOwned>,
    /// Optional description.
    pub description: Option<String>,
    /// Optional localized names.
    pub names: BTreeMap<String, String>,
}

impl SpaceTypeDeclaration {
    /// Build a validated declaration from Jacquard's parsed Lexicon AST.
    pub fn from_lexicon(nsid: &str, document: &LexiconDoc<'_>) -> Result<Self> {
        let nsid = parse_nsid("space type", nsid)?;
        if document.id.as_ref() != nsid.as_str() {
            return Err(PermissionedError::InvalidDeclaration(
                "document id does not match requested NSID".into(),
            ));
        }
        let Some(LexUserType::Space(LexSpace {
            key: Some(key),
            name: Some(name),
            collections,
            description,
            name_lang,
        })) = document.defs.get("main")
        else {
            return Err(PermissionedError::InvalidDeclaration(
                "defs.main is not a complete space declaration".into(),
            ));
        };
        let key = parse_rkey("key", key.as_ref())?;
        let graphemes = name.chars().count();
        if !(1..=64).contains(&graphemes) {
            return Err(PermissionedError::InvalidDeclaration(
                "name must contain 1..=64 characters".into(),
            ));
        }
        if collections.is_empty() {
            return Err(PermissionedError::InvalidDeclaration(
                "collections cannot be empty".into(),
            ));
        }
        let mut validated_collections = Vec::with_capacity(collections.len());
        for collection in collections {
            let collection = parse_nsid("collection", collection.as_ref())?;
            if validated_collections
                .iter()
                .any(|existing| existing == &collection)
            {
                return Err(PermissionedError::InvalidDeclaration(
                    "duplicate collection".into(),
                ));
            }
            validated_collections.push(collection);
        }
        Ok(Self {
            nsid,
            key,
            name: name.to_string(),
            collections: validated_collections,
            description: description.as_ref().map(ToString::to_string),
            names: name_lang
                .as_ref()
                .into_iter()
                .flat_map(|names| names.iter())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
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
    /// Canonical permissioned-space URI.
    pub space: SpaceUri,
    /// DID of the record author.
    pub author: DidOwned,
    /// Host-assigned commit revision.
    pub rev: Tid,
}

/// Generated wire representation of a permissioned commit.
pub use jacquard_api::com_atproto::space::SignedCommit;

fn commit_bytes(field: &'static str, value: &Bytes) -> Result<[u8; 32]> {
    value
        .as_ref()
        .try_into()
        .map_err(|_| PermissionedError::InvalidCommit(format!("{field} must contain 32 bytes")))
}

fn commit_fields(commit: &SignedCommit) -> Result<([u8; 32], [u8; 32], [u8; 32])> {
    if commit.ver != 1 {
        return Err(PermissionedError::InvalidCommit(
            "version or revision mismatch".into(),
        ));
    }
    Ok((
        commit_bytes("hash", &commit.hash)?,
        commit_bytes("ikm", &commit.ikm)?,
        commit_bytes("mac", &commit.mac)?,
    ))
}

/// Sign a commit with fresh random IKM.
pub fn sign_commit(
    hash: [u8; 32],
    context: &CommitContext,
    key: &ed25519_dalek::SigningKey,
) -> Result<SignedCommit> {
    let mut ikm = [0; 32];
    rand::thread_rng().fill_bytes(&mut ikm);
    sign_commit_with_ikm(hash, context, key, ikm)
}

/// Sign a commit with explicit IKM, used for independent conformance vectors.
pub fn sign_commit_with_ikm(
    hash: [u8; 32],
    context: &CommitContext,
    key: &ed25519_dalek::SigningKey,
    ikm: [u8; 32],
) -> Result<SignedCommit> {
    let context_bytes = encode_commit_context(context, &ikm)?;
    let mac = compute_mac(&ikm, &context_bytes, &hash);
    Ok(SignedCommit {
        hash: Bytes::copy_from_slice(&hash),
        ikm: Bytes::copy_from_slice(&ikm),
        mac: Bytes::copy_from_slice(&mac),
        rev: context.rev.clone(),
        sig: Bytes::copy_from_slice(&key.sign(&context_bytes).to_bytes()),
        ver: 1,
        extra_data: None,
    })
}

/// Verify the commit version, revision, MAC, and Ed25519 context signature.
pub fn verify_commit(
    commit: &SignedCommit,
    context: &CommitContext,
    key: &VerifyingKey,
) -> Result<()> {
    let (hash, ikm, mac) = commit_fields(commit)?;
    if commit.rev != context.rev {
        return Err(PermissionedError::InvalidCommit(
            "version or revision mismatch".into(),
        ));
    }
    let context_bytes = encode_commit_context(context, &ikm)?;
    if compute_mac(&ikm, &context_bytes, &hash) != mac {
        return Err(PermissionedError::InvalidCommit("MAC mismatch".into()));
    }
    let signature = Signature::from_slice(&commit.sig)
        .map_err(|_| PermissionedError::InvalidCommit("invalid signature bytes".into()))?;
    key.verify(&context_bytes, &signature)
        .map_err(|_| PermissionedError::InvalidCommit("signature mismatch".into()))
}

/// Encode a generated permissioned commit as canonical DAG-CBOR.
pub fn commit_to_cbor(commit: &SignedCommit) -> Result<Vec<u8>> {
    serde_ipld_dagcbor::to_vec(commit)
        .map_err(|error| PermissionedError::Serialization(error.to_string()))
}

/// Decode and validate a generated DAG-CBOR permissioned commit.
pub fn commit_from_cbor(bytes: &[u8]) -> Result<SignedCommit> {
    let commit: SignedCommit = serde_ipld_dagcbor::from_slice(bytes)
        .map_err(|error| PermissionedError::Serialization(error.to_string()))?;
    commit_fields(&commit)?;
    Ok(commit)
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
    /// Create a new record.
    Create {
        /// Record URI to create.
        uri: SpaceUri,
        /// CID of the new record block.
        cid: CidOwned,
        /// Canonical record bytes.
        value: Bytes,
    },
    /// Replace an existing record.
    Update {
        /// Record URI to replace.
        uri: SpaceUri,
        /// CID currently stored at the URI.
        prev: CidOwned,
        /// CID of the replacement block.
        cid: CidOwned,
        /// Replacement record bytes.
        value: Bytes,
    },
    /// Delete an existing record.
    Delete {
        /// Record URI to remove.
        uri: SpaceUri,
        /// CID currently stored at the URI.
        prev: CidOwned,
    },
}

/// Current in-memory values used by pure write validation and conformance tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteState {
    /// Current records keyed by canonical space URI.
    pub records: HashMap<SpaceUri, RecordValue>,
    /// Current LtHash accumulator.
    pub lthash: LtHash,
}

/// A current record value and CID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordValue {
    /// Current record block CID.
    pub cid: CidOwned,
    /// Canonical record bytes.
    pub value: Bytes,
    /// Record author identifier.
    pub author: Identifier,
}

/// One write's result in input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteResult {
    /// Canonical URI affected by the write.
    pub uri: SpaceUri,
    /// New record CID, or `None` for deletion.
    pub cid: Option<CidOwned>,
}

/// Oplog action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OplogAction {
    /// A record was created.
    Create,
    /// A record was replaced.
    Update,
    /// A record was deleted.
    Delete,
}

/// Reference-shaped oplog entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OplogEntry {
    /// Space containing the operation.
    pub space: SpaceUri,
    /// Batch revision.
    pub rev: Tid,
    /// Zero-based operation index within the batch.
    pub idx: u32,
    /// Operation kind.
    pub action: OplogAction,
    /// Canonical record URI.
    pub uri: SpaceUri,
    /// Record collection NSID.
    pub collection: NsidOwned,
    /// Record key.
    pub rkey: RkeyOwned,
    /// Resulting record CID, absent for deletion.
    pub cid: Option<CidOwned>,
    /// Previous record CID, absent for creation.
    pub prev: Option<CidOwned>,
}

/// Atomic result of validating and applying a write group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWritesResult {
    /// Revision shared by all writes in the batch.
    pub revision: Tid,
    /// Per-write results in input order.
    pub results: Vec<WriteResult>,
    /// Oplog entries in input order.
    pub oplog: Vec<OplogEntry>,
    /// Candidate LtHash after applying the batch.
    pub lthash: LtHash,
}

/// Validate and apply a complete group to a cloned state, swapping only on success.
pub fn apply_writes(
    state: &mut WriteState,
    space: &SpaceUri,
    revision: &Tid,
    operations: &[WriteOperation],
) -> Result<ApplyWritesResult> {
    if operations.len() > 200 {
        return Err(PermissionedError::InvalidWrite("maximum 200 writes".into()));
    }
    let space_authority = space.did_authority();
    if space.is_record() {
        return Err(PermissionedError::InvalidWrite(
            "space URI must not name a record".into(),
        ));
    }
    let mut candidate = state.clone();
    let mut results = Vec::with_capacity(operations.len());
    let mut oplog = Vec::with_capacity(operations.len());
    let mut batch_author: Option<DidOwned> = None;
    for (idx, operation) in operations.iter().enumerate() {
        let (uri, action, cid, prev, value) = match operation {
            WriteOperation::Create { uri, cid, value } => (
                uri,
                OplogAction::Create,
                Some(cid.clone()),
                None,
                Some(value.clone()),
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
            ),
            WriteOperation::Delete { uri, prev } => {
                (uri, OplogAction::Delete, None, Some(prev.clone()), None)
            }
        };
        if !uri.is_record() {
            return Err(PermissionedError::InvalidWrite(
                "write URI must name a record".into(),
            ));
        }
        if uri.did_authority().as_str() != space_authority.as_str()
            || uri.space_type().as_str() != space.space_type().as_str()
            || uri.skey().as_str() != space.skey().as_str()
        {
            return Err(PermissionedError::InvalidWrite(
                "write URI targets a different space".into(),
            ));
        }
        let author = uri
            .did_author()
            .ok_or_else(|| PermissionedError::InvalidWrite("missing author DID".into()))?;
        let author = parse_did("record author", author.as_str())?;
        if batch_author.as_ref().is_some_and(|value| value != &author) {
            return Err(PermissionedError::InvalidWrite(
                "all writes must target the same author DID".into(),
            ));
        }
        batch_author.get_or_insert_with(|| author.clone());
        let key = uri.clone();
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
        match (&cid, &value) {
            (Some(cid_value), Some(record_value)) => {
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
                        author: Identifier::new_owned(author.as_str()).map_err(|_| {
                            PermissionedError::InvalidWrite("invalid author DID".into())
                        })?,
                    },
                );
            }
            (None, None) => {
                candidate.records.remove(&key);
            }
            _ => {
                return Err(PermissionedError::InvalidWrite(
                    "write CID/value shape mismatch".into(),
                ));
            }
        }
        let (collection, rkey) = (path_collection(uri)?, path_rkey(uri)?);
        oplog.push(OplogEntry {
            space: space.clone(),
            rev: revision.clone(),
            idx: idx as u32,
            action,
            uri: uri.clone(),
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
        revision: revision.clone(),
        results,
        oplog,
        lthash: candidate.lthash.clone(),
    };
    *state = candidate;
    Ok(result)
}

fn path_collection(uri: &SpaceUri) -> Result<NsidOwned> {
    uri.collection()
        .map(|collection| parse_nsid("collection", collection.as_str()))
        .transpose()?
        .ok_or_else(|| PermissionedError::InvalidWrite("missing collection".into()))
}
fn path_rkey(uri: &SpaceUri) -> Result<RkeyOwned> {
    uri.rkey()
        .map(|rkey| parse_rkey("rkey", rkey.as_str()))
        .transpose()?
        .ok_or_else(|| PermissionedError::InvalidWrite("missing rkey".into()))
}

/// Parse the normative slash-separated cursor.
pub fn parse_cursor(cursor: &str) -> Result<(Tid, u32)> {
    let (rev, idx) = cursor
        .split_once('/')
        .ok_or_else(|| PermissionedError::InvalidWrite("cursor must use rev/idx".into()))?;
    if cursor.matches('/').count() != 1 {
        return Err(PermissionedError::InvalidWrite("invalid cursor".into()));
    }
    let rev = parse_tid("cursor revision", rev)
        .map_err(|_| PermissionedError::InvalidWrite("invalid cursor".into()))?;
    let idx = idx
        .parse::<u32>()
        .map_err(|_| PermissionedError::InvalidWrite("invalid cursor index".into()))?;
    Ok((rev, idx))
}

/// Format a cursor using the normative slash separator.
pub fn format_cursor(rev: &Tid, idx: u32) -> SmolStr {
    SmolStr::new(format!("{rev}/{idx}"))
}

/// A page of ordered operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OplogPage {
    /// Operations in revision/index order.
    pub ops: Vec<OplogEntry>,
    /// Cursor for the next page, if one exists.
    pub cursor: Option<SmolStr>,
    /// Terminal commit included on a short page.
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
    if limit == 0 {
        return Err(PermissionedError::InvalidWrite(
            "limit must be greater than zero".into(),
        ));
    }
    let start = if let Some(cursor) = cursor {
        Some(parse_cursor(cursor)?)
    } else if let Some(since) = since {
        Some((
            parse_tid("since", since).map_err(|_| PermissionedError::SinceUnavailable)?,
            u32::MAX,
        ))
    } else {
        None
    };
    let mut ordered = entries.to_vec();
    ordered.sort_by(|left, right| left.rev.cmp(&right.rev).then(left.idx.cmp(&right.idx)));
    if let Some((start_rev, _)) = &start {
        if since.is_some()
            && !ordered.is_empty()
            && !ordered.iter().any(|entry| entry.rev == *start_rev)
        {
            return Err(PermissionedError::SinceUnavailable);
        }
    }
    let filtered: Vec<_> = ordered
        .into_iter()
        .filter(|entry| {
            start.as_ref().is_none_or(|(start_rev, start_idx)| {
                (entry.rev.as_str() > start_rev.as_str())
                    || (entry.rev == *start_rev && entry.idx > *start_idx)
            })
        })
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

/// JWT `typ` for a delegation token.
pub const DELEGATION_TOKEN_TYP: &str = "atproto-space-delegation+jwt";
/// JWT `typ` for a reusable DPoP-bound space credential.
pub const SPACE_CREDENTIAL_TYP: &str = "atproto-space-credential+jwt";
/// JWT `typ` for an OAuth client attestation.
pub const CLIENT_ATTESTATION_TYP: &str = "atproto-client-attestation+jwt";
/// Permitted clock skew when evaluating token timestamps.
pub const CLOCK_SKEW_SEC: i64 = 5;

/// Wire-shaped credential header and claims after JWT signature verification.
///
/// Issuer and subject remain strings because client attestations use a client
/// ID URL for both, while delegation and space credentials use DID/space values.
/// The JWT `typ` lives in the protected header (per the reference
/// implementation), so it is supplied by the caller alongside the claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialClaims {
    /// Token issuer: a DID except for client attestations.
    pub iss: SmolStr,
    /// Token subject: a space URI except for client attestations.
    pub sub: SmolStr,
    /// Optional JWT audience.
    pub aud: Option<SmolStr>,
    /// Issued-at timestamp in Unix seconds.
    pub iat: i64,
    /// Expiry timestamp in Unix seconds.
    pub exp: i64,
    /// Unique token identifier.
    pub jti: SmolStr,
    /// Optional DPoP JWK thumbprint, flattened from the wire's `cnf.jkt`.
    #[serde(rename = "cnf")]
    pub cnf_jkt: Option<CnfJkt>,
}

/// The `cnf` (confirmation) claim carrying the DPoP key thumbprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CnfJkt {
    /// RFC 8747 JWK thumbprint of the bound proof key.
    pub jkt: SmolStr,
}

impl CredentialClaims {
    fn validate_common(&self, now: i64) -> Result<()> {
        if self.jti.is_empty() {
            return Err(PermissionedError::InvalidCredential(
                "token type or jti mismatch".into(),
            ));
        }
        if self.exp < now - CLOCK_SKEW_SEC || self.iat > now + CLOCK_SKEW_SEC {
            return Err(PermissionedError::InvalidCredential(
                "token time claim mismatch".into(),
            ));
        }
        Ok(())
    }

    /// Validate a delegation token and consume its `jti` in the replay set.
    pub fn validate_delegation(
        &self,
        now: i64,
        expected_issuer: &Did<&str>,
        expected_subject: &AtSpaceUri<&str>,
        replay: &mut BTreeSet<SmolStr>,
    ) -> Result<()> {
        self.validate_common(now)?;
        let expected_audience = format!("{}#atproto_space_host", expected_subject.did_authority());
        if self.iss != expected_issuer.as_str()
            || self.sub != expected_subject.as_str()
            || self.aud.as_deref() != Some(expected_audience.as_str())
            || self.cnf_jkt.is_some()
            || !replay.insert(self.jti.clone())
        {
            return Err(PermissionedError::InvalidCredential(
                "delegation claims or replay mismatch".into(),
            ));
        }
        Ok(())
    }

    /// Validate an authority-issued DPoP-bound space credential.
    pub fn validate_space_credential(
        &self,
        now: i64,
        expected_issuer: &Did<&str>,
        expected_subject: &AtSpaceUri<&str>,
        required_jkt: &str,
    ) -> Result<()> {
        self.validate_common(now)?;
        if self.iss != expected_issuer.as_str()
            || self.sub != expected_subject.as_str()
            || self.aud.is_some()
            || self.cnf_jkt.as_ref().map(|c| c.jkt.as_str()) != Some(required_jkt)
        {
            return Err(PermissionedError::InvalidCredential(
                "space credential claims mismatch".into(),
            ));
        }
        Ok(())
    }

    /// Validate a client attestation and consume its `jti` in the replay set.
    pub fn validate_client_attestation(
        &self,
        now: i64,
        expected_client_id: &str,
        expected_authority: &Did<&str>,
        replay: &mut BTreeSet<SmolStr>,
    ) -> Result<()> {
        self.validate_common(now)?;
        let client_id = url::Url::parse(expected_client_id).map_err(|_| {
            PermissionedError::InvalidCredential("client_id must be an absolute URL".into())
        })?;
        if client_id.scheme() != "https" || client_id.host_str().is_none() {
            return Err(PermissionedError::InvalidCredential(
                "client_id must be an HTTPS URL".into(),
            ));
        }
        let expected_audience = format!("{expected_authority}#atproto_space_host");
        if self.iss != expected_client_id
            || self.sub != expected_client_id
            || self.aud.as_deref() != Some(expected_audience.as_str())
            || self.cnf_jkt.is_some()
            || !replay.insert(self.jti.clone())
        {
            return Err(PermissionedError::InvalidCredential(
                "client attestation claims or replay mismatch".into(),
            ));
        }
        Ok(())
    }
}

/// DPoP proof claims after JWT signature verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpopProof {
    /// Unique proof identifier used for replay detection.
    pub jti: SmolStr,
    /// HTTP method bound to the proof.
    pub htm: SmolStr,
    /// HTTP target URL bound to the proof.
    pub htu: SmolStr,
    /// Base64url SHA-256 digest of the credential.
    pub ath: SmolStr,
    /// Issued-at timestamp in Unix seconds.
    pub iat: i64,
}

/// Validate DPoP method, URL, credential hash, age, and replay through a caller-owned set.
pub fn verify_dpop(
    proof: &DpopProof,
    method: &str,
    url: &str,
    credential: &[u8],
    now: i64,
    replay: &mut BTreeSet<SmolStr>,
) -> Result<()> {
    let proof_htu = normalize_htu(&proof.htu)?;
    let request_htu = normalize_htu(url)?;
    if proof.htm != method
        || proof_htu != request_htu
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

/// Normalize a DPoP target URL to its WHATWG origin and pathname.
///
/// Parsing canonicalizes the hostname and elides default ports, matching
/// `new URL(url).origin + pathname` in the reference implementation.
pub fn normalize_htu(url: &str) -> Result<String> {
    let parsed = url::Url::parse(url).map_err(|_| {
        PermissionedError::InvalidCredential("DPoP htu must be an absolute URL".into())
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(PermissionedError::InvalidCredential(
            "DPoP htu must use HTTP(S) with an authority".into(),
        ));
    }
    Ok(format!(
        "{}{}",
        parsed.origin().ascii_serialization(),
        parsed.path()
    ))
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

/// Ordered two-root permissioned CAR representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionedCar {
    /// Signed commit root first.
    pub roots: [CarCid; 2],
    /// Commit, index, then records in canonical order.
    pub blocks: Vec<(CarCid, Bytes)>,
}

/// Immutable result returned after full CAR verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRepoSnapshot {
    roots: [CarCid; 2],
    commit: SignedCommit,
    index: Vec<(SmolStr, CarCid)>,
    records: Vec<(SmolStr, CarCid, Bytes)>,
}

impl PermissionedCar {
    /// Construct a two-root CAR representation after structural CID validation.
    pub fn new(roots: [CarCid; 2], blocks: Vec<(CarCid, Bytes)>) -> Result<Self> {
        let car = Self { roots, blocks };
        car.validate_structure()?;
        Ok(car)
    }

    /// Return a roots-only CAR representation.
    pub fn exclude_values(&self) -> Self {
        Self {
            roots: self.roots,
            blocks: self.blocks.iter().take(2).cloned().collect(),
        }
    }

    fn validate_structure(&self) -> Result<()> {
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

    /// Fully authenticate the commit, DRISL index, and optional record blocks.
    ///
    /// The expected space and author are supplied out of band, as in the
    /// reference consumer. A roots-only CAR is accepted when `expect_values` is
    /// false; otherwise every index entry must have exactly one matching block.
    pub fn validate(
        &self,
        space: &SpaceUri,
        author: &DidOwned,
        key: &VerifyingKey,
        expect_values: bool,
    ) -> Result<ValidatedRepoSnapshot> {
        self.validate_structure()?;

        let commit = commit_from_cbor(&self.blocks[0].1)
            .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
        let commit_bytes = commit_to_cbor(&commit)
            .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
        if commit_bytes.as_slice() != self.blocks[0].1.as_ref() {
            return Err(PermissionedError::InvalidCar(
                "commit block is not canonical DAG-CBOR".into(),
            ));
        }
        let context = CommitContext {
            space: space.clone(),
            author: author.clone(),
            rev: commit.rev.clone(),
        };
        verify_commit(&commit, &context, key)
            .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;

        let decoded: BTreeMap<SmolStr, CidLink<SmolStr>> =
            serde_ipld_dagcbor::from_slice(&self.blocks[1].1)
                .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
        let canonical = serde_ipld_dagcbor::to_vec(&decoded)
            .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
        if canonical.as_slice() != self.blocks[1].1.as_ref() {
            return Err(PermissionedError::InvalidCar(
                "index block is not canonical DAG-CBOR".into(),
            ));
        }

        let mut index = decoded
            .into_iter()
            .map(|(path, cid)| {
                let (collection, rkey) = path.split_once('/').ok_or_else(|| {
                    PermissionedError::InvalidCar("invalid index record path".into())
                })?;
                if path.matches('/').count() != 1 {
                    return Err(PermissionedError::InvalidCar(
                        "invalid index record path".into(),
                    ));
                }
                parse_nsid("index collection", collection)
                    .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
                parse_rkey("index rkey", rkey)
                    .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
                let cid = cid
                    .to_ipld()
                    .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
                Ok((path, cid))
            })
            .collect::<Result<Vec<_>>>()?;
        index.sort_by(|left, right| {
            left.0
                .len()
                .cmp(&right.0.len())
                .then_with(|| left.0.as_bytes().cmp(right.0.as_bytes()))
        });

        let mut lthash = LtHash::default();
        for (path, cid) in &index {
            lthash.add(&format!("{path}/{cid}"));
        }
        if commit.hash.as_ref() != lthash.digest().as_slice() {
            return Err(PermissionedError::InvalidCar(
                "index does not match commit hash".into(),
            ));
        }

        let record_blocks = &self.blocks[2..];
        if expect_values && record_blocks.len() != index.len() {
            return Err(PermissionedError::InvalidCar(
                "record block count does not match index".into(),
            ));
        }
        if !expect_values && !record_blocks.is_empty() && record_blocks.len() != index.len() {
            return Err(PermissionedError::InvalidCar(
                "partial record block stream".into(),
            ));
        }

        let mut records = Vec::with_capacity(record_blocks.len());
        for ((path, expected_cid), (actual_cid, bytes)) in index.iter().zip(record_blocks) {
            if actual_cid != expected_cid {
                return Err(PermissionedError::InvalidCar(
                    "record blocks do not follow index order".into(),
                ));
            }
            let value: Data<SmolStr> = serde_ipld_dagcbor::from_slice(bytes)
                .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
            let canonical = serde_ipld_dagcbor::to_vec(&value)
                .map_err(|error| PermissionedError::InvalidCar(error.to_string()))?;
            if canonical.as_slice() != bytes.as_ref() {
                return Err(PermissionedError::InvalidCar(
                    "record block is not canonical DAG-CBOR".into(),
                ));
            }
            records.push((path.clone(), *actual_cid, bytes.clone()));
        }

        Ok(ValidatedRepoSnapshot {
            roots: self.roots,
            commit,
            index,
            records,
        })
    }
}

impl ValidatedRepoSnapshot {
    /// Return the ordered commit and index roots.
    pub fn roots(&self) -> &[CarCid; 2] {
        &self.roots
    }

    /// Return the authenticated signed commit.
    pub fn commit(&self) -> &SignedCommit {
        &self.commit
    }

    /// Return the canonical ordered DRISL index.
    pub fn index(&self) -> &[(SmolStr, CarCid)] {
        &self.index
    }

    /// Return verified record blocks in index order.
    pub fn records(&self) -> &[(SmolStr, CarCid, Bytes)] {
        &self.records
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
            space: SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap(),
            author: DidOwned::new_owned("did:plc:author").unwrap(),
            rev: Tid::new("3jzfcijpj2m2a").unwrap(),
        };
        let key = SigningKey::from_bytes(&[7; 32]);
        let commit = sign_commit_with_ikm([9; 32], &context, &key, [0x20; 32]).unwrap();
        verify_commit(&commit, &context, &key.verifying_key()).unwrap();
        let bytes = commit_to_cbor(&commit).unwrap();
        assert_eq!(commit_from_cbor(&bytes).unwrap(), commit);
    }
    #[test]
    fn signed_commit_all_mutation_paths_fail() {
        let context = CommitContext {
            space: SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap(),
            author: DidOwned::new_owned("did:plc:author").unwrap(),
            rev: Tid::new("3jzfcijpj2m2a").unwrap(),
        };
        let key = SigningKey::from_bytes(&[7; 32]);
        let commit = sign_commit_with_ikm([9; 32], &context, &key, [0x20; 32]).unwrap();

        let mut changed_context = context.clone();
        changed_context.space =
            SpaceUri::new_owned("at://did:plc:other/space/com.example.type/demo").unwrap();
        assert!(verify_commit(&commit, &changed_context, &key.verifying_key()).is_err());
        changed_context = context.clone();
        changed_context.author = DidOwned::new_owned("did:plc:other").unwrap();
        assert!(verify_commit(&commit, &changed_context, &key.verifying_key()).is_err());
        changed_context = context.clone();
        changed_context.rev = Tid::new("3jzfcijpj2m2b").unwrap();
        assert!(verify_commit(&commit, &changed_context, &key.verifying_key()).is_err());

        for mutate in [
            |value: &mut SignedCommit| {
                let mut bytes = value.hash.to_vec();
                bytes[0] ^= 1;
                value.hash = Bytes::from(bytes);
            },
            |value: &mut SignedCommit| {
                let mut bytes = value.ikm.to_vec();
                bytes[0] ^= 1;
                value.ikm = Bytes::from(bytes);
            },
            |value: &mut SignedCommit| {
                let mut bytes = value.mac.to_vec();
                bytes[0] ^= 1;
                value.mac = Bytes::from(bytes);
            },
            |value: &mut SignedCommit| {
                let mut bytes = value.sig.to_vec();
                bytes[0] ^= 1;
                value.sig = Bytes::from(bytes);
            },
            |value: &mut SignedCommit| value.ver += 1,
        ] {
            let mut changed = commit.clone();
            mutate(&mut changed);
            assert!(verify_commit(&changed, &context, &key.verifying_key()).is_err());
        }
        let mut changed = commit.clone();
        changed.rev = Tid::new("3jzfcijpj2m2b").unwrap();
        assert!(verify_commit(&changed, &context, &key.verifying_key()).is_err());
    }
    #[test]
    fn write_group_is_atomic_and_cursor_uses_slash() {
        let uri = SpaceUri::new_owned(
            "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record/r",
        )
        .unwrap();
        let mut state = WriteState::default();
        let ops = [WriteOperation::Create {
            uri: uri.clone(),
            cid: CidOwned::from_str("bafybeigdyrzt5o5p4s5x6f7g8h9j0k1l2m3n4o5p6q7r8s9t0u").unwrap(),
            value: Bytes::from_static(b"{}"),
        }];
        let space = SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap();
        let revision = Tid::new("3jzfcijpj2m2a").unwrap();
        let result = apply_writes(&mut state, &space, &revision, &ops).unwrap();
        assert_eq!(result.oplog[0].idx, 0);
        assert_eq!(format_cursor(&revision, 0), "3jzfcijpj2m2a/0");
    }

    fn credential_claims(
        iss: &str,
        sub: &str,
        aud: Option<&str>,
        cnf_jkt: Option<&str>,
    ) -> CredentialClaims {
        CredentialClaims {
            iss: SmolStr::new(iss),
            sub: SmolStr::new(sub),
            aud: aud.map(SmolStr::new),
            iat: 100,
            exp: 160,
            jti: SmolStr::new_static("unique"),
            cnf_jkt: cnf_jkt.map(|jkt| CnfJkt {
                jkt: SmolStr::new(jkt),
            }),
        }
    }

    #[test]
    fn credentials_validate_each_kind_and_replay() {
        let space = SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap();
        let user = DidOwned::new_owned("did:plc:user").unwrap();
        let authority = DidOwned::new_owned("did:plc:space").unwrap();
        let user_ref = Did::new(user.as_str()).unwrap();
        let authority_ref = Did::new(authority.as_str()).unwrap();
        let space_ref = AtSpaceUri::new(space.as_str()).unwrap();
        let mut replay = BTreeSet::new();
        let delegation = credential_claims(
            user.as_str(),
            space.as_str(),
            Some("did:plc:space#atproto_space_host"),
            None,
        );
        delegation
            .validate_delegation(100, &user_ref, &space_ref, &mut replay)
            .unwrap();
        assert!(
            delegation
                .validate_delegation(100, &user_ref, &space_ref, &mut replay)
                .is_err()
        );

        let credential =
            credential_claims(authority.as_str(), space.as_str(), None, Some("thumbprint"));
        credential
            .validate_space_credential(100, &authority_ref, &space_ref, "thumbprint")
            .unwrap();
        assert!(
            credential
                .validate_space_credential(100, &user_ref, &space_ref, "thumbprint")
                .is_err()
        );

        let client_id = "https://client.example/metadata.json";
        let attestation = credential_claims(
            client_id,
            client_id,
            Some("did:plc:space#atproto_space_host"),
            None,
        );
        let mut replay = BTreeSet::new();
        attestation
            .validate_client_attestation(100, client_id, &authority_ref, &mut replay)
            .unwrap();
        assert!(
            attestation
                .validate_client_attestation(
                    100,
                    "did:plc:not-a-client-url",
                    &authority_ref,
                    &mut BTreeSet::new(),
                )
                .is_err()
        );
    }

    #[test]
    fn credentials_apply_five_second_expiry_grace() {
        let space = SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap();
        let authority = DidOwned::new_owned("did:plc:space").unwrap();
        let authority_ref = Did::new(authority.as_str()).unwrap();
        let space_ref = AtSpaceUri::new(space.as_str()).unwrap();
        let mut credential =
            credential_claims(authority.as_str(), space.as_str(), None, Some("thumbprint"));
        credential.exp = 100;
        assert!(
            credential
                .validate_space_credential(105, &authority_ref, &space_ref, "thumbprint")
                .is_ok()
        );
        assert!(
            credential
                .validate_space_credential(106, &authority_ref, &space_ref, "thumbprint")
                .is_err()
        );
    }

    fn verified_car_fixture() -> (PermissionedCar, SpaceUri, DidOwned, SigningKey) {
        let space = SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap();
        let author = DidOwned::new_owned("did:plc:author").unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let record = serde_ipld_dagcbor::to_vec(&BTreeMap::from([(
            SmolStr::new_static("text"),
            SmolStr::new_static("hello"),
        )]))
        .unwrap();
        let record_cid = crate::mst::util::compute_cid(&record).unwrap();
        let path = SmolStr::new_static("com.example.record/r");
        let index = BTreeMap::from([(path.clone(), CidLink::<SmolStr>::ipld(record_cid))]);
        let index_bytes = serde_ipld_dagcbor::to_vec(&index).unwrap();
        let index_cid = crate::mst::util::compute_cid(&index_bytes).unwrap();
        let mut lthash = LtHash::default();
        lthash.add(&format!("{path}/{record_cid}"));
        let context = CommitContext {
            space: space.clone(),
            author: author.clone(),
            rev: Tid::new("3jzfcijpj2m2a").unwrap(),
        };
        let commit = sign_commit_with_ikm(lthash.digest(), &context, &key, [0x20; 32]).unwrap();
        let commit_bytes = commit_to_cbor(&commit).unwrap();
        let commit_cid = crate::mst::util::compute_cid(&commit_bytes).unwrap();
        let car = PermissionedCar::new(
            [commit_cid, index_cid],
            vec![
                (commit_cid, Bytes::from(commit_bytes)),
                (index_cid, Bytes::from(index_bytes)),
                (record_cid, Bytes::from(record)),
            ],
        )
        .unwrap();
        (car, space, author, key)
    }

    #[test]
    fn permissioned_car_full_semantic_verification() {
        let (car, space, author, key) = verified_car_fixture();
        let snapshot = car
            .validate(&space, &author, &key.verifying_key(), true)
            .unwrap();
        assert_eq!(snapshot.index().len(), 1);
        assert_eq!(snapshot.records().len(), 1);
        let mut hash = LtHash::default();
        hash.add(&format!(
            "{}/{}",
            snapshot.index()[0].0,
            snapshot.index()[0].1
        ));
        assert_eq!(snapshot.commit().hash.as_ref(), hash.digest().as_slice());

        let roots_only = car.exclude_values();
        assert!(
            roots_only
                .validate(&space, &author, &key.verifying_key(), false)
                .is_ok()
        );
        assert!(
            roots_only
                .validate(&space, &author, &key.verifying_key(), true)
                .is_err()
        );
    }

    #[test]
    fn permissioned_car_rejects_wrong_context_and_record_order() {
        let (car, space, author, key) = verified_car_fixture();
        let other_author = DidOwned::new_owned("did:plc:other").unwrap();
        assert!(
            car.validate(&space, &other_author, &key.verifying_key(), true)
                .is_err()
        );
        let wrong_key = SigningKey::from_bytes(&[8; 32]);
        assert!(
            car.validate(&space, &author, &wrong_key.verifying_key(), true)
                .is_err()
        );

        let mut truncated = car.clone();
        truncated.blocks.pop();
        assert!(
            truncated
                .validate(&space, &author, &key.verifying_key(), true)
                .is_err()
        );
        let mut wrong_record = car;
        wrong_record.blocks[2].1 = Bytes::from_static(b"bad");
        assert!(wrong_record.validate_structure().is_err());
    }
}
