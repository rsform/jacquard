//! Error types for repository operations

use std::error::Error;
use std::fmt;

/// Boxed error type for error sources
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Result type alias for repository operations
pub type Result<T> = std::result::Result<T, RepoError>;

/// Repository operation error with rich diagnostics
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub struct RepoError {
    kind: RepoErrorKind,
    #[source]
    source: Option<BoxError>,
    #[help]
    help: Option<String>,
    context: Option<String>,
}

/// Error categories for repository operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoErrorKind {
    /// Storage operation failed
    Storage,
    /// Invalid MST structure
    InvalidMst,
    /// Invalid commit structure
    InvalidCommit,
    /// Invalid key format
    InvalidKey,
    /// Invalid CID
    InvalidCid,
    /// Resource not found
    NotFound,
    /// Cryptographic operation failed
    Crypto,
    /// Serialization/deserialization failed
    Serialization,
    /// Data too large (exceeds protocol limits)
    TooLarge,
    /// CAR file operation failed
    Car,
    /// I/O error
    Io,
}

impl RepoError {
    /// Create a new error with the given kind and optional source
    pub fn new(kind: RepoErrorKind, source: Option<BoxError>) -> Self {
        Self {
            kind,
            source,
            help: None,
            context: None,
        }
    }

    /// Add a help message to the error
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add context information to the error
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Get the error kind
    pub fn kind(&self) -> &RepoErrorKind {
        &self.kind
    }

    // Constructors for different error kinds

    /// Create a storage error
    pub fn storage(source: impl Error + Send + Sync + 'static) -> Self {
        Self::new(RepoErrorKind::Storage, Some(Box::new(source)))
    }

    /// Create an invalid MST error
    pub fn invalid_mst(msg: impl Into<String>) -> Self {
        Self::new(RepoErrorKind::InvalidMst, Some(msg.into().into()))
            .with_help("MST nodes must follow protocol structure")
    }

    /// Create an invalid commit error
    pub fn invalid_commit(msg: impl Into<String>) -> Self {
        Self::new(RepoErrorKind::InvalidCommit, Some(msg.into().into()))
    }

    /// Create an invalid key error
    pub fn invalid_key(key: impl Into<String>) -> Self {
        Self::new(RepoErrorKind::InvalidKey, None)
            .with_help("MST keys must match [a-zA-Z0-9._:~-]+, max 256 bytes")
            .with_context(format!("key: {}", key.into()))
    }

    /// Create an invalid CID error
    pub fn invalid_cid(msg: impl Into<String>) -> Self {
        Self::new(RepoErrorKind::InvalidCid, Some(msg.into().into()))
    }

    /// Create a not found error
    pub fn not_found(resource: &str, id: impl fmt::Display) -> Self {
        Self::new(RepoErrorKind::NotFound, None)
            .with_context(format!("{} not found: {}", resource, id))
    }

    /// Create an already exists error
    pub fn already_exists(resource: &str, id: impl fmt::Display) -> Self {
        Self::new(RepoErrorKind::InvalidMst, None)
            .with_context(format!("{} already exists: {}", resource, id))
    }

    /// Create a crypto error
    pub fn crypto(source: impl Error + Send + Sync + 'static) -> Self {
        Self::new(RepoErrorKind::Crypto, Some(Box::new(source)))
    }

    /// Create a serialization error
    pub fn serialization(source: impl Error + Send + Sync + 'static) -> Self {
        Self::new(RepoErrorKind::Serialization, Some(Box::new(source)))
    }

    /// Create a too large error
    pub fn too_large(what: &str, size: usize, max: usize) -> Self {
        Self::new(RepoErrorKind::TooLarge, None)
            .with_context(format!("{} is {} bytes, max {}", what, size, max))
            .with_help("See sync v1.1 protocol limits")
    }

    /// Create a CAR file error
    pub fn car(source: impl Error + Send + Sync + 'static) -> Self {
        Self::new(RepoErrorKind::Car, Some(Box::new(source)))
    }

    /// Create a CAR parse error (alias for car)
    pub fn car_parse(source: impl Error + Send + Sync + 'static) -> Self {
        Self::car(source).with_context("Failed to parse CAR file".to_string())
    }

    /// Create an I/O error
    pub fn io(source: impl Error + Send + Sync + 'static) -> Self {
        Self::new(RepoErrorKind::Io, Some(Box::new(source)))
    }

    /// Create a generic invalid error
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new(RepoErrorKind::InvalidMst, Some(msg.into().into()))
    }
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)?;

        if let Some(ctx) = &self.context {
            write!(f, ": {}", ctx)?;
        }

        if let Some(src) = &self.source {
            write!(f, ": {}", src)?;
        }

        Ok(())
    }
}

// Internal granular errors

/// MST-specific errors
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum MstError {
    /// Empty key not allowed
    #[error("Empty key not allowed")]
    EmptyKey,

    /// Key too long
    #[error("Key too long: {len} bytes (max {max})")]
    KeyTooLong {
        /// Actual key length
        len: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Invalid key characters
    #[error("Invalid key characters: {key}")]
    InvalidKeyChars {
        /// The invalid key
        key: String,
    },

    /// Node structure invalid
    #[error("Node structure invalid: {0}")]
    InvalidNode(String),

    /// Serialization failed
    #[error("Serialization failed")]
    Serialization(#[source] BoxError),
}

impl From<MstError> for RepoError {
    fn from(e: MstError) -> Self {
        match e {
            MstError::EmptyKey => RepoError::invalid_key(""),
            MstError::KeyTooLong { len, max } => {
                RepoError::invalid_key(format!("length {}/{}", len, max))
            }
            MstError::InvalidKeyChars { key } => RepoError::invalid_key(key),
            MstError::InvalidNode(msg) => RepoError::invalid_mst(msg),
            MstError::Serialization(e) => RepoError::new(RepoErrorKind::Serialization, Some(e)),
        }
    }
}

/// Commit-specific errors
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum CommitError {
    /// Invalid commit version
    #[error("Invalid commit version: {0}")]
    InvalidVersion(i64),

    /// Invalid signature format
    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),

    /// Signature verification failed
    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    /// Invalid key format
    #[error("Invalid key format: {0}")]
    InvalidKey(String),

    /// Unsupported key type
    #[error("Unsupported key type: {0}")]
    UnsupportedKeyType(u64),

    /// Serialization failed
    #[error("Serialization failed")]
    Serialization(#[source] BoxError),
}

impl From<CommitError> for RepoError {
    fn from(e: CommitError) -> Self {
        match e {
            CommitError::InvalidVersion(v) => {
                RepoError::invalid_commit(format!("unsupported version {}", v))
            }
            CommitError::InvalidSignature(msg) => {
                RepoError::new(RepoErrorKind::Crypto, Some(msg.into()))
                    .with_context("invalid signature format".to_string())
            }
            CommitError::SignatureVerificationFailed => RepoError::new(RepoErrorKind::Crypto, None)
                .with_context("signature verification failed".to_string()),
            CommitError::InvalidKey(msg) => RepoError::new(RepoErrorKind::Crypto, Some(msg.into()))
                .with_context("invalid key format".to_string()),
            CommitError::UnsupportedKeyType(code) => RepoError::new(RepoErrorKind::Crypto, None)
                .with_context(format!("unsupported key type: 0x{:x}", code)),
            CommitError::Serialization(e) => RepoError::new(RepoErrorKind::Serialization, Some(e)),
        }
    }
}

/// Diff-specific errors
#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    /// Too many operations
    #[error("Too many operations: {count} (max {max})")]
    TooManyOps {
        /// Actual operation count
        count: usize,
        /// Maximum allowed operations
        max: usize,
    },

    /// Diff too large
    #[error("Diff too large: {size} bytes (max {max})")]
    TooLarge {
        /// Actual size
        size: usize,
        /// Maximum size
        max: usize,
    },
}

impl From<DiffError> for RepoError {
    fn from(e: DiffError) -> Self {
        match e {
            DiffError::TooManyOps { count, max } => {
                RepoError::too_large("diff operation count", count, max)
            }
            DiffError::TooLarge { size, max } => RepoError::too_large("diff size", size, max),
        }
    }
}

/// Proof verification errors
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ProofError {
    /// CAR file has no root CID
    #[error("CAR file has no root CID")]
    #[diagnostic(
        code(proof::no_root),
        help("CAR files for proofs must have exactly one root CID pointing to the commit")
    )]
    NoRoot,

    /// Commit block not found in CAR
    #[error("Commit block not found in CAR")]
    #[diagnostic(
        code(proof::commit_not_found),
        help("The CAR root CID must point to a valid commit block")
    )]
    CommitNotFound,

    /// DID mismatch between commit and expected
    #[error("DID mismatch: commit has {commit_did}, expected {expected_did}")]
    #[diagnostic(
        code(proof::did_mismatch),
        help("The commit must be signed by the expected DID")
    )]
    DidMismatch {
        /// DID in the commit
        commit_did: String,
        /// Expected DID
        expected_did: String,
    },

    /// Signature verification failed
    #[error("Signature verification failed")]
    #[diagnostic(
        code(proof::signature_failed),
        help("The commit signature must be valid for the provided public key")
    )]
    SignatureVerificationFailed {
        /// Underlying crypto error
        #[source]
        source: CommitError,
    },

    /// MST root block missing from CAR
    #[error("MST root block missing from CAR: {cid}")]
    #[diagnostic(
        code(proof::missing_mst_block),
        help("All MST blocks along the proof path must be included in the CAR file")
    )]
    MissingMstBlock {
        /// The missing CID
        cid: String,
    },

    /// Invalid commit structure
    #[error("Invalid commit structure: {0}")]
    #[diagnostic(code(proof::invalid_commit))]
    InvalidCommit(String),

    /// CAR parsing failed
    #[error("CAR parsing failed")]
    #[diagnostic(code(proof::car_parse_failed))]
    CarParseFailed {
        /// Underlying error
        #[source]
        source: BoxError,
    },

    /// Commit deserialization failed
    #[error("Commit deserialization failed")]
    #[diagnostic(code(proof::commit_deserialize_failed))]
    CommitDeserializeFailed {
        /// Underlying error
        #[source]
        source: BoxError,
    },
}

impl From<ProofError> for RepoError {
    fn from(e: ProofError) -> Self {
        match &e {
            ProofError::NoRoot => RepoError::invalid("CAR file has no root CID"),
            ProofError::CommitNotFound => {
                RepoError::new(RepoErrorKind::NotFound, Some(Box::new(e)))
            }
            ProofError::DidMismatch { .. } => {
                RepoError::new(RepoErrorKind::InvalidCommit, Some(Box::new(e)))
            }
            ProofError::SignatureVerificationFailed { .. } => {
                RepoError::new(RepoErrorKind::Crypto, Some(Box::new(e)))
            }
            ProofError::MissingMstBlock { .. } => {
                RepoError::new(RepoErrorKind::NotFound, Some(Box::new(e)))
            }
            ProofError::InvalidCommit(_) => {
                RepoError::new(RepoErrorKind::InvalidCommit, Some(Box::new(e)))
            }
            ProofError::CarParseFailed { .. } => {
                RepoError::new(RepoErrorKind::Car, Some(Box::new(e)))
            }
            ProofError::CommitDeserializeFailed { .. } => {
                RepoError::new(RepoErrorKind::Serialization, Some(Box::new(e)))
            }
        }
    }
}
