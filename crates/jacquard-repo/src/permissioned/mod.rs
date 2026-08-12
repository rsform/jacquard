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
use hmac::{Hmac, Mac};
use jacquard_common::SmolStr;
use jacquard_common::types::aturi::AtSpaceUri;
use jacquard_common::types::cid::Cid as AtCid;
use jacquard_common::types::did::Did;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::Rkey;
use jacquard_common::types::tid::Tid;

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

fn invalid_component(field: &'static str, value: impl Into<String>) -> PermissionedError {
    PermissionedError::InvalidComponent {
        field,
        value: value.into(),
    }
}

fn space_uri(uri: impl AsRef<str>) -> Result<SpaceUri> {
    SpaceUri::new_owned(uri)
        .map_err(|error| invalid_component("permissioned URI", error.to_string()))
}

fn parse_did(field: &'static str, value: impl AsRef<str>) -> Result<DidOwned> {
    DidOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_identifier(field: &'static str, value: impl AsRef<str>) -> Result<Identifier> {
    Identifier::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_nsid(field: &'static str, value: impl AsRef<str>) -> Result<NsidOwned> {
    NsidOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_rkey(field: &'static str, value: impl AsRef<str>) -> Result<RkeyOwned> {
    RkeyOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
}

fn parse_cid(field: &'static str, value: impl AsRef<str>) -> Result<CidOwned> {
    CidOwned::from_str(value.as_ref()).map_err(|_| invalid_component(field, value.as_ref()))
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
    pub space: SpaceUri,
    pub author: DidOwned,
    pub rev: Tid,
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
    pub rev: Tid,
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
        uri: SpaceUri,
        cid: CidOwned,
        value: Bytes,
    },
    Update {
        uri: SpaceUri,
        prev: CidOwned,
        cid: CidOwned,
        value: Bytes,
    },
    Delete {
        uri: SpaceUri,
        prev: CidOwned,
    },
}

/// Current in-memory values used by pure write validation and conformance tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteState {
    pub records: HashMap<SpaceUri, RecordValue>,
    pub lthash: LtHash,
}

/// A current record value and CID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordValue {
    pub cid: CidOwned,
    pub value: Bytes,
    pub author: Identifier,
}

/// One write's result in input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteResult {
    pub uri: SpaceUri,
    pub cid: Option<CidOwned>,
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
    pub space: SpaceUri,
    pub rev: Tid,
    pub idx: u32,
    pub action: OplogAction,
    pub uri: SpaceUri,
    pub collection: NsidOwned,
    pub rkey: RkeyOwned,
    pub cid: Option<CidOwned>,
    pub prev: Option<CidOwned>,
}

/// Atomic result of validating and applying a write group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWritesResult {
    pub revision: Tid,
    pub results: Vec<WriteResult>,
    pub oplog: Vec<OplogEntry>,
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
    let mut candidate = state.clone();
    let mut results = Vec::with_capacity(operations.len());
    let mut oplog = Vec::with_capacity(operations.len());
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
        let author = uri
            .author()
            .ok_or_else(|| PermissionedError::InvalidWrite("missing author DID".into()))?;
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
                        author: Identifier::new_owned(author.as_str()).map_err(|_| {
                            PermissionedError::InvalidWrite("invalid author DID".into())
                        })?,
                    },
                );
            } else {
                candidate.records.remove(&key);
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
    pub ops: Vec<OplogEntry>,
    pub cursor: Option<SmolStr>,
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
            && ordered
                .first()
                .is_some_and(|entry| entry.rev.as_str() > start_rev.as_str())
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
    pub iss: DidOwned,
    pub sub: SpaceUri,
    pub aud: Option<Identifier>,
    pub iat: i64,
    pub exp: i64,
    pub jti: SmolStr,
    pub cnf_jkt: Option<SmolStr>,
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
        if self.sub.as_str() != expected_subject || self.exp < now || self.iat > now + 5 {
            return Err(PermissionedError::InvalidCredential(
                "subject or time claim mismatch".into(),
            ));
        }
        if self.aud.as_ref().map(AsRef::as_ref) != expected_audience {
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
    pub jti: SmolStr,
    pub htm: SmolStr,
    pub htu: SmolStr,
    pub ath: SmolStr,
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
    pub space_type: NsidOwned,
    pub authority: Option<SmolStr>,
    pub skey: Option<RkeyOwned>,
    pub collection: Vec<SmolStr>,
    pub action: Vec<SmolStr>,
    pub manage: Vec<SmolStr>,
}

impl SpacePermission {
    /// Validate a permission-set entry; wildcard space types are forbidden in sets.
    pub fn validate(&self, namespace: &str) -> Result<()> {
        if !self.space_type.as_str().starts_with(namespace) {
            return Err(PermissionedError::InvalidCredential(
                "permission outside namespace".into(),
            ));
        }
        for collection in &self.collection {
            if collection != "*" {
                parse_nsid("collection", collection)?;
            }
        }
        if let Some(authority) = &self.authority {
            if authority != "self" && authority != "*" {
                parse_did("authority", authority)?;
            }
        }
        Ok(())
    }
    /// Match independently against a requested space and action.
    pub fn matches(&self, space_type: &str, collection: &str, action: &str) -> bool {
        (self.space_type.as_str() == space_type || self.space_type.as_str() == "*")
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
    pub roots: [CarCid; 2],
    /// Commit, index, then records in canonical order.
    pub blocks: Vec<(CarCid, Bytes)>,
}

/// Immutable result returned after CAR verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRepoSnapshot {
    /// Verified roots.
    pub roots: [CarCid; 2],
    /// Verified blocks in input order.
    pub blocks: Vec<(CarCid, Bytes)>,
}

impl PermissionedCar {
    /// Construct and validate a two-root CAR representation.
    pub fn new(roots: [CarCid; 2], blocks: Vec<(CarCid, Bytes)>) -> Result<Self> {
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
    pub fn roots(&self) -> &[CarCid; 2] {
        &self.roots
    }

    /// Return verified blocks without exposing mutable state.
    pub fn blocks(&self) -> &[(CarCid, Bytes)] {
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
            space: SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap(),
            author: DidOwned::new_owned("did:plc:author").unwrap(),
            rev: Tid::new("3jzfcijpj2m2a").unwrap(),
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
            space: SpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap(),
            author: DidOwned::new_owned("did:plc:author").unwrap(),
            rev: Tid::new("3jzfcijpj2m2a").unwrap(),
        };
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut commit = SignedCommit::sign_with_ikm([9; 32], &context, &key, [0x20; 32]).unwrap();
        commit.hash[0] ^= 1;
        assert!(commit.verify(&context, &key.verifying_key()).is_err());
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
}
